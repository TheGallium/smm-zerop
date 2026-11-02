use std::{collections::HashSet, time::Duration};

use anyhow::Context;
use jiff::{Timestamp, tz::TimeZone};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::time::{self, MissedTickBehavior};
use tracing::info;

use crate::entities::smm2_level::Smm2Level;

#[derive(Debug, Deserialize)]
struct Page {
    levels: Vec<UpstreamLevel>,
    total: u64,
}

#[derive(Debug, Deserialize)]
struct UpstreamLevel {
    code: String,
    name: String,
    description: Option<String>,
    uploaded_at: Timestamp,
    clear_check_ms: i64,
    attempts: i64,
    plays: i64,
    tags: Vec<String>,
    game_style_name: String,
    likes: i64,
    boos: i64,
    is_unprocessed_clear: bool,
    is_cleared: bool,
    is_deleted: bool,
    num_comments: i64,
    overworld: UpstreamArea,
    clear_condition_id: Option<i64>,
    clear_condition_magnitude: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct UpstreamArea {
    theme_name: String,
}

fn normalized_id(id: &str) -> String {
    id.trim().replace('-', "").to_lowercase()
}

fn normalized_tags(tags: Vec<String>) -> Vec<String> {
    let mut tags: Vec<String> = tags
        .iter()
        .filter(|t| t.as_str() != "None")
        .map(|t| normalize_tag_name(t))
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

fn normalize_tag_name(name: &str) -> String {
    name.trim().replace(' ', "_").to_lowercase()
}

fn non_zero(value: Option<i64>) -> Option<i64> {
    value.filter(|v| *v != 0)
}

impl UpstreamLevel {
    fn is_listable(&self) -> bool {
        !(self.is_cleared || self.is_unprocessed_clear || self.is_deleted)
    }
}

impl From<UpstreamLevel> for Smm2Level {
    fn from(value: UpstreamLevel) -> Self {
        Smm2Level {
            id: normalized_id(&value.code),
            year: value.uploaded_at.to_zoned(TimeZone::UTC).year() as i64,
            title: value.name,
            description: value.description.filter(|d| !d.is_empty()),
            uploaded_at: value.uploaded_at,
            clearcheck_ms: value.clear_check_ms,
            attempts: value.attempts,
            footprints: value.plays,
            likes: value.likes,
            boos: value.boos,
            comments: value.num_comments,
            clear_condition: non_zero(value.clear_condition_id),
            clear_condition_magnitude: non_zero(value.clear_condition_magnitude),
            style: value.game_style_name,
            theme: normalize_tag_name(&value.overworld.theme_name),
            tags: normalized_tags(value.tags),
        }
    }
}

async fn fetch_page(client: &reqwest::Client, offset: u64) -> anyhow::Result<Page> {
    let url = format!("https://mm2list.cyzon.us/c/all?format=json&offset={offset}");

    client
        .get(&url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("requesting {url}"))?
        .json()
        .await
        .with_context(|| format!("decoding response of {url}"))
}

#[tracing::instrument(skip(own_db))]
pub async fn run(own_db: PgPool) -> anyhow::Result<()> {
    let client = reqwest::Client::builder()
        .user_agent("smm-uncleared.com (ping @skyschub on Discord)")
        .timeout(Duration::from_secs(60))
        .build()?;

    let mut throttle = time::interval(Duration::from_secs(1));
    throttle.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let level_blocklist: HashSet<String> =
        sqlx::query_scalar!("SELECT level_id FROM level_blocklist WHERE game = 'smm2'")
            .fetch_all(&own_db)
            .await?
            .into_iter()
            .collect();

    let mut db_transaction = own_db.begin().await?;

    info!("deleting current levels...");
    sqlx::query!("DELETE FROM levels_smm2")
        .execute(&mut *db_transaction)
        .await?;

    info!("streaming levels...");
    let mut levels_count = 0;
    let mut offset = 0;
    loop {
        throttle.tick().await;
        let page = fetch_page(&client, offset).await?;
        info!(offset, total = page.total, "fetched page");

        let page_is_empty = page.levels.is_empty();
        offset += page.levels.len() as u64;

        for upstream_level in page.levels.into_iter().filter(|l| l.is_listable()) {
            let level = Smm2Level::from(upstream_level);
            if level_blocklist.contains(&level.id) {
                continue;
            }

            levels_count += level.store(&mut *db_transaction).await?.rows_affected();
        }

        if page_is_empty || offset >= page.total {
            break;
        }
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
