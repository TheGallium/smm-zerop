use std::collections::HashSet;

use futures_util::TryStreamExt;
use jiff::{Span, Timestamp, civil, tz::TimeZone};
use sqlx::PgPool;
use tiberius::{ColumnData, FromSql};
use tracing::info;

use crate::entities::smm2_level::Smm2Level;

macro_rules! expect_not_null {
    ($value:expr, $field:expr) => {
        $value
            .get($field)
            .expect(&format!("{} IS NOT NULL", $field))
    };
    ($value:expr, $field:expr, $type:ty) => {
        $value
            .get::<$type, &str>($field)
            .expect(&format!("{} IS NOT NULL", $field))
    };
}

/// A `datetime2` column, interpreted as UTC, to hack jiff support into
/// tiberius. I'm sure there's a saner way, but tiberius is unmaintained
/// anyway, maybe, so for now, I'll just ignore it.
struct UtcDatetime2(Timestamp);

impl<'a> FromSql<'a> for UtcDatetime2 {
    fn from_sql(value: &'a ColumnData<'static>) -> tiberius::Result<Option<Self>> {
        let ColumnData::DateTime2(dt) = value else {
            return Err(tiberius::error::Error::Conversion(
                format!("cannot interpret {value:?} as a datetime2").into(),
            ));
        };
        let Some(dt) = dt else {
            return Ok(None);
        };

        let conversion_error =
            |e: jiff::Error| tiberius::error::Error::Conversion(e.to_string().into());

        // `days` counts from 0001-01-01; `increments` are in units of
        // 10^-scale seconds.
        let date = civil::date(1, 1, 1)
            .checked_add(
                Span::new()
                    .try_days(dt.date().days())
                    .map_err(conversion_error)?,
            )
            .map_err(conversion_error)?;
        let nanos = dt.time().increments() * 10u64.pow(9 - dt.time().scale() as u32);
        let timestamp = date
            .to_datetime(civil::Time::midnight())
            .checked_add(
                Span::new()
                    .try_nanoseconds(nanos as i64)
                    .map_err(conversion_error)?,
            )
            .and_then(|datetime| datetime.to_zoned(TimeZone::UTC))
            .map_err(conversion_error)?
            .timestamp();

        Ok(Some(Self(timestamp)))
    }
}

fn normalized_id(id: &str) -> String {
    id.trim().replace('-', "").to_lowercase()
}

fn normalized_tags(tag1: Option<&str>, tag2: Option<&str>) -> Vec<String> {
    let mut tags = vec![tag1, tag2];
    tags.sort();
    tags.dedup();

    tags.iter()
        .filter_map(|o| *o)
        .filter(|t| *t != "None")
        .map(normalize_tag_name)
        .collect()
}

fn normalize_tag_name(name: &str) -> String {
    name.trim().replace(' ', "_").to_lowercase()
}

impl From<tiberius::Row> for Smm2Level {
    fn from(value: tiberius::Row) -> Self {
        let id: &str = expect_not_null!(value, "id");

        // Cryptan stored the upload datetimes without offset. It's kinda
        // irrelevant for our use-case, so I'll just assume they're always UTC.
        let UtcDatetime2(uploaded_at) = expect_not_null!(value, "date");

        let title: &str = expect_not_null!(value, "name");
        let description = value.get::<&str, &str>("description").map(|s| s.to_owned());

        let clearcheck_ms: i32 = expect_not_null!(value, "upload_time");
        let attempts: i32 = expect_not_null!(value, "attempts");
        let footprints: i32 = expect_not_null!(value, "footprints");

        // Please do not ask me why those fields are strings...
        let likes: i64 = expect_not_null!(value, "likes", &str)
            .parse::<i64>()
            .expect("field should be a numeric value");
        let boos: i64 = expect_not_null!(value, "boos", &str)
            .parse::<i64>()
            .expect("field should be a numeric value");
        let comments: i64 = expect_not_null!(value, "comments", &str)
            .parse::<i64>()
            .expect("field should be a numeric value");

        let clear_condition = match value.get::<i32, &str>("clear_condition") {
            None => None,
            Some(0) => None,
            Some(i) => Some(i as i64),
        };
        let clear_condition_magnitude = match value.get::<i32, &str>("clear_condition_magnitude") {
            None => None,
            Some(0) => None,
            Some(i) => Some(i as i64),
        };

        let style: &str = expect_not_null!(value, "style");
        let theme: &str = expect_not_null!(value, "theme");
        let tags = normalized_tags(value.get("tag1"), value.get("tag2"));

        Smm2Level {
            id: normalized_id(id),
            year: uploaded_at.to_zoned(TimeZone::UTC).year() as i64,
            title: title.to_owned(),
            description,
            uploaded_at,
            clearcheck_ms: clearcheck_ms as i64,
            attempts: attempts as i64,
            footprints: footprints as i64,
            likes,
            boos,
            comments,
            clear_condition,
            clear_condition_magnitude,
            style: style.to_owned(),
            theme: normalize_tag_name(theme),
            tags,
        }
    }
}

#[tracing::instrument(skip(upstream_db, own_db))]
pub async fn run(
    upstream_db: &mut tiberius::Client<tokio_util::compat::Compat<tokio::net::TcpStream>>,
    own_db: PgPool,
) -> anyhow::Result<()> {
    let level_blocklist: HashSet<String> =
        sqlx::query_scalar!("SELECT level_id FROM level_blocklist WHERE game = 'smm2'")
            .fetch_all(&own_db)
            .await?
            .iter()
            .cloned()
            .collect();

    let mut db_transaction = own_db.begin().await?;

    info!("deleting current levels...");
    sqlx::query!("DELETE FROM levels_smm2")
        .execute(&mut *db_transaction)
        .await?;

    info!("streaming levels...");
    let mut levels_count = 0;
    let mut rows = upstream_db
        .simple_query("SELECT * FROM v_Uncleared")
        .await?
        .into_row_stream();
    while let Some(row) = rows.try_next().await? {
        let level: Smm2Level = row.into();

        if level_blocklist.contains(&level.id) {
            continue;
        }

        level.store(&mut *db_transaction).await?;
        levels_count += 1;
    }

    if levels_count > 0 {
        info!("committing...");
        db_transaction.commit().await?;
        info!("done, imported {} levels!", levels_count);
    } else {
        info!("somehow got no levels, rolling back...");
        db_transaction.rollback().await?;
        info!("done!");
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use tiberius::time::{Date, DateTime2, Time};

    use super::*;

    #[test]
    fn datetime2_is_decoded_as_utc() {
        let column = ColumnData::DateTime2(Some(DateTime2::new(
            Date::new(738949),
            Time::new(256891230000, 7),
        )));
        let UtcDatetime2(timestamp) = UtcDatetime2::from_sql(&column).unwrap().unwrap();
        assert_eq!(timestamp.to_string(), "2024-03-05T07:08:09.123Z");
    }

    #[test]
    fn null_datetime2_is_none() {
        let column = ColumnData::DateTime2(None);
        assert!(UtcDatetime2::from_sql(&column).unwrap().is_none());
    }
}
