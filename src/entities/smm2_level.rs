use std::vec;

use jiff::Timestamp;
use jiff_sqlx::ToSqlx;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgExecutor, Postgres, QueryBuilder, postgres::PgQueryResult};

use crate::components::deserializers::{empty_string_as_none, empty_string_as_none_enum};

macro_rules! push_optional_filter {
    ($builder:expr, $field_name:expr, $check:expr) => {
        push_optional_filter!($builder, $field_name, $check, "");
    };
    ($builder:expr, $field_name:expr, $check_pre:expr, $check_post:expr) => {
        if let Some(val) = $field_name {
            $builder.push($check_pre);
            $builder.push_bind(val);
            $builder.push($check_post);
        }
    };
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct Smm2Level {
    pub id: String,
    pub year: i64,

    pub title: String,
    pub description: Option<String>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub uploaded_at: Timestamp,
    pub clearcheck_ms: i64,

    pub attempts: i64,
    pub footprints: i64,
    pub likes: i64,
    pub boos: i64,
    pub comments: i64,

    pub clear_condition: Option<i64>,
    pub clear_condition_magnitude: Option<i64>,

    pub style: String,
    pub theme: String,
    pub tags: Vec<String>,
}

impl Smm2Level {
    pub async fn store<'a, Executor: PgExecutor<'a>>(
        &self,
        executor: Executor,
    ) -> Result<PgQueryResult, sqlx::Error> {
        sqlx::query!(
            "INSERT INTO levels_smm2 (
                id,
                year,
                title,
                description,
                uploaded_at,
                clearcheck_ms,
                attempts,
                footprints,
                likes,
                boos,
                comments,
                clear_condition,
                clear_condition_magnitude,
                style,
                theme,
                tags
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
            ON CONFLICT DO NOTHING",
            self.id,
            self.year,
            self.title,
            self.description,
            self.uploaded_at.to_sqlx() as _,
            self.clearcheck_ms,
            self.attempts,
            self.footprints,
            self.likes,
            self.boos,
            self.comments,
            self.clear_condition,
            self.clear_condition_magnitude,
            self.style,
            self.theme,
            &self.tags,
        )
        .execute(executor)
        .await
    }

    pub async fn get_random_level<'a, Executor: PgExecutor<'a>>(
        executor: Executor,
        params: &FilterParams,
    ) -> Result<Option<Smm2Level>, sqlx::Error> {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT
                id,
                year,
                title,
                description,
                uploaded_at,
                clearcheck_ms,
                attempts,
                footprints,
                likes,
                boos,
                comments,
                clear_condition,
                clear_condition_magnitude,
                style,
                theme,
                tags
            FROM levels_smm2
            WHERE 1 = 1",
        );

        push_optional_filter!(query, params.year, " AND year = ");
        push_optional_filter!(query, params.min_attempts, " AND attempts >= ");
        push_optional_filter!(query, params.max_attempts, " AND attempts <= ");
        push_optional_filter!(query, params.min_footprints, " AND footprints >= ");
        push_optional_filter!(query, params.max_footprints, " AND footprints <= ");
        push_optional_filter!(query, params.min_clearcheck_ms, " AND clearcheck_ms >= ");
        push_optional_filter!(query, params.max_clearcheck_ms, " AND clearcheck_ms <= ");
        push_optional_filter!(query, &params.style, " AND style =  ");
        push_optional_filter!(query, &params.theme, " AND theme =  ");
        push_optional_filter!(query, &params.tag, " AND ", " = ANY(tags)");

        if let Some(cc_filter) = &params.clear_condition_group {
            match cc_filter.id_list() {
                None => {
                    query.push(" AND clear_condition IS NULL");
                }
                Some(ids) => {
                    query.push(format!(
                        " AND clear_condition IN({})",
                        ids.iter()
                            .map(|i| i.to_string())
                            .collect::<Vec<String>>()
                            .join(",")
                    ));
                }
            }
        }

        query.push(" ORDER BY random() LIMIT 1");

        query
            .build_query_as::<Smm2Level>()
            .fetch_optional(executor)
            .await
    }

    pub async fn id_exists<'a, Executor: PgExecutor<'a>>(
        executor: Executor,
        level_id: &str,
    ) -> bool {
        sqlx::query!("SELECT id FROM levels_smm2 WHERE id = $1 LIMIT 1", level_id)
            .fetch_optional(executor)
            .await
            .is_ok_and(|r| r.is_some())
    }

    pub fn normalized_internal_level_id(raw_id: &str) -> String {
        raw_id.trim().replace('-', "").to_lowercase()
    }

    pub fn formatted_level_id(raw_id: &str) -> String {
        raw_id
            .to_uppercase()
            .chars()
            .collect::<Vec<char>>()
            .chunks(3)
            .map(|c| c.iter().collect::<String>())
            .collect::<Vec<String>>()
            .join("-")
    }

    pub fn clear_condition_text(id: i64, magnitude: Option<i64>) -> String {
        if let Some(cc_label) = clear_condition_label(id) {
            if let Some(magnitude) = magnitude {
                let plural_suffix = match magnitude {
                    1 => "",
                    _ => "s",
                };

                cc_label
                    .replace("(n)", &magnitude.to_string())
                    .replace("(s)", plural_suffix)
            } else {
                cc_label.to_string()
            }
        } else {
            "Unknown Clear Condition :(".to_string()
        }
    }
}

impl From<Smm2Level> for minijinja::Value {
    fn from(value: Smm2Level) -> Self {
        minijinja::Value::from(minijinja::value::Serde(value))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "UPPERCASE")]
pub enum Style {
    SMB1,
    SMB3,
    SMW,
    NSMBU,
    SM3DW,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum Theme {
    Airship,
    Castle,
    Desert,
    Forest,
    GhostHouse,
    Overworld,
    Sky,
    Snow,
    Underground,
}

#[derive(Clone, Debug, Deserialize, Serialize, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum Tag {
    Art,
    AutoMario,
    Autoscroll,
    BossBattle,
    Link,
    MultiplayerVersus,
    Music,
    PuzzleSolving,
    Shooter,
    ShortAndSweet,
    SinglePlayer,
    Speedrun,
    Standard,
    Technical,
    Themed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearConditionGroup {
    None,
    NoJumping,
    NoDamage,
    DefeatingEnemies,
    PowerupFinish,
    HoldingActivating,
    Collecting,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FilterParams {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub year: Option<i64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub min_attempts: Option<i64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub max_attempts: Option<i64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub min_footprints: Option<i64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub max_footprints: Option<i64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub min_clearcheck_ms: Option<i64>,

    #[serde(default, deserialize_with = "empty_string_as_none")]
    pub max_clearcheck_ms: Option<i64>,

    #[serde(default, deserialize_with = "empty_string_as_none_enum")]
    pub clear_condition_group: Option<ClearConditionGroup>,

    #[serde(default, deserialize_with = "empty_string_as_none_enum")]
    pub style: Option<Style>,

    #[serde(default, deserialize_with = "empty_string_as_none_enum")]
    pub theme: Option<Theme>,

    #[serde(default, deserialize_with = "empty_string_as_none_enum")]
    pub tag: Option<Tag>,
}

impl From<FilterParams> for minijinja::Value {
    fn from(value: FilterParams) -> Self {
        minijinja::Value::from(minijinja::value::Serde(value))
    }
}

impl ClearConditionGroup {
    fn id_list(&self) -> Option<Vec<i64>> {
        match self {
            Self::None => None,
            Self::NoJumping => Some(vec![137525990]),
            Self::NoDamage => Some(vec![375673178]),
            Self::DefeatingEnemies => Some(vec![
                199585683, 272349836, 751004331, 947659466, 998904081, 1008094897, 1061233896,
                1062253843, 1079889509, 1080535886, 1182464856, 1221661152, 1259427138, 1268255615,
                1279580818, 1344044032, 1425973877, 1431944825, 1446467058, 1510495760, 1665820273,
                1724036958, 1730095541, 1839897151, 1969299694, 2035052211, 2038503215, 2048033177,
                2131209407, 2139645066, 2259346429, 2694559007, 2746139466, 3174413484, 3346433512,
                3353006607, 3392229961, 3437308486, 3459144213, 3466227835, 3481362698, 3748075486,
                3797704544, 3824561269, 3833342952, 3842179831, 3974581191, 4042480826, 4117878280,
                4269094462, 4293354249,
            ]),
            Self::PowerupFinish => Some(vec![
                426197923, 436833616, 713979835, 744927294, 900050759, 976173462, 994686866,
                1151250770, 1283945123, 1780278293, 2076496776, 2089161429, 2111528319, 2549654281,
                2855236681, 3036298571, 3074433106, 3146932243, 3206222275, 3314955857, 3342591980,
                3513732174, 3649647177, 3725246406, 3730243509, 3977257962, 4153835197, 4172105156,
                4209535561,
            ]),
            Self::HoldingActivating => Some(vec![
                1051433633, 1219761531, 1429902736, 1676924210, 1715960804,
            ]),
            Self::Collecting => Some(vec![
                1656179347, 2749601092, 3348058176, 3874680510, 4116396131, 4122555074,
            ]),
        }
    }
}

fn clear_condition_label(id: i64) -> Option<&'static str> {
    match id {
        0 => None,
        137525990 => Some("Reach the goal without landing after leaving the ground."),
        199585683 => Some("Reach the goal after defeating (n) Mechakoopa(s)."),
        272349836 => Some("Reach the goal after defeating (n) Cheep Cheep(s)."),
        375673178 => Some("Reach the goal without taking damage."),
        426197923 => Some("Reach the goal as Boomerang Mario."),
        436833616 => Some("Reach the goal while wearing a Shoe."),
        713979835 => Some("Reach the goal as Fire Mario."),
        744927294 => Some("Reach the goal as Frog Mario."),
        751004331 => Some("Reach the goal after defeating (n) Larry(s)."),
        900050759 => Some("Reach the goal as Raccoon Mario."),
        947659466 => Some("Reach the goal after defeating (n) Blooper(s)."),
        976173462 => Some("Reach the goal as Propeller Mario."),
        994686866 => Some("Reach the goal while wearing a Propeller Box."),
        998904081 => Some("Reach the goal after defeating (n) Spike(s)."),
        1008094897 => Some("Reach the goal after defeating (n) Boom Boom(s)."),
        1051433633 => Some("Reach the goal while holding a Koopa Shell."),
        1061233896 => Some("Reach the goal after defeating (n) Porcupuffer(s)."),
        1062253843 => Some("Reach the goal after defeating (n) Charvaargh(s)."),
        1079889509 => Some("Reach the goal after defeating (n) Bullet Bill(s)."),
        1080535886 => Some("Reach the goal after defeating (n) Bully/Bullies."),
        1151250770 => Some("Reach the goal while wearing a Goomba Mask."),
        1182464856 => Some("Reach the goal after defeating (n) Hop-Chops."),
        1219761531 => Some(
            "Reach the goal while holding a Red POW Block. OR Reach the goal after activating (n) Red POW Block(s).",
        ),
        1221661152 => Some("Reach the goal after defeating (n) Bob-omb(s)."),
        1259427138 => Some("Reach the goal after defeating (n) Spiny/Spinies."),
        1268255615 => Some("Reach the goal after defeating (n) Bowser(s)/Meowser(s)."),
        1279580818 => Some("Reach the goal after defeating (n) Ant Trooper(s)."),
        1283945123 => Some("Reach the goal on a Lakitu's Cloud."),
        1344044032 => Some("Reach the goal after defeating (n) Boo(s)."),
        1425973877 => Some("Reach the goal after defeating (n) Roy(s)."),
        1429902736 => Some("Reach the goal while holding a Trampoline."),
        1431944825 => Some("Reach the goal after defeating (n) Morton(s)."),
        1446467058 => Some("Reach the goal after defeating (n) Fish Bone(s)."),
        1510495760 => Some("Reach the goal after defeating (n) Monty Mole(s)."),
        1656179347 => Some("Reach the goal after picking up (n) 1-Up Mushroom(s)."),
        1665820273 => Some("Reach the goal after defeating (n) Hammer Bro(s)."),
        1676924210 => Some(
            "Reach the goal after hitting (n) P Switch(es). OR Reach the goal while holding a P Switch.",
        ),
        1715960804 => Some(
            "Reach the goal after activating (n) POW Block(s). OR Reach the goal while holding a POW Block.",
        ),
        1724036958 => Some("Reach the goal after defeating (n) Angry Sun(s)."),
        1730095541 => Some("Reach the goal after defeating (n) Pokey(s)."),
        1780278293 => Some("Reach the goal as Superball Mario."),
        1839897151 => Some("Reach the goal after defeating (n) Pom Pom(s)."),
        1969299694 => Some("Reach the goal after defeating (n) Peepa(s)."),
        2035052211 => Some("Reach the goal after defeating (n) Lakitu(s)."),
        2038503215 => Some("Reach the goal after defeating (n) Lemmy(s)."),
        2048033177 => Some("Reach the goal after defeating (n) Lava Bubble(s)."),
        2076496776 => Some("Reach the goal while wearing a Bullet Bill Mask."),
        2089161429 => Some("Reach the goal as Big Mario."),
        2111528319 => Some("Reach the goal as Cat Mario."),
        2131209407 => Some("Reach the goal after defeating (n) Goomba(s)/Galoomba(s)."),
        2139645066 => Some("Reach the goal after defeating (n) Thwomp(s)."),
        2259346429 => Some("Reach the goal after defeating (n) Iggy(s)."),
        2549654281 => Some("Reach the goal while wearing a Dry Bones Shell."),
        2694559007 => Some("Reach the goal after defeating (n) Sledge Bro(s)."),
        2746139466 => Some("Reach the goal after defeating (n) Rocky Wrench(es)."),
        2749601092 => Some("Reach the goal after grabbing (n) 50-Coin(s)."),
        2855236681 => Some("Reach the goal as Flying Squirrel Mario."),
        3036298571 => Some("Reach the goal as Buzzy Mario."),
        3074433106 => Some("Reach the goal as Builder Mario."),
        3146932243 => Some("Reach the goal as Cape Mario."),
        3174413484 => Some("Reach the goal after defeating (n) Wendy(s)."),
        3206222275 => Some("Reach the goal while wearing a Cannon Box."),
        3314955857 => Some("Reach the goal as Link."),
        3342591980 => Some("Reach the goal while you have Super Star invincibility."),
        3346433512 => Some("Reach the goal after defeating (n) Goombrat(s)/Goombud(s)."),
        3348058176 => Some("Reach the goal after grabbing (n) 10-Coin(s)."),
        3353006607 => Some("Reach the goal after defeating (n) Buzzy Beetle(s)."),
        3392229961 => Some("Reach the goal after defeating (n) Bowser Jr.(s)."),
        3437308486 => Some("Reach the goal after defeating (n) Koopa Troopa(s)."),
        3459144213 => Some("Reach the goal after defeating (n) Chain Chomp(s)."),
        3466227835 => Some("Reach the goal after defeating (n) Muncher(s)."),
        3481362698 => Some("Reach the goal after defeating (n) Wiggler(s)."),
        3513732174 => Some("Reach the goal as SMB2 Mario."),
        3649647177 => Some("Reach the goal in a Koopa Clown Car/Junior Clown Car."),
        3725246406 => Some("Reach the goal as Spiny Mario."),
        3730243509 => Some("Reach the goal in a Koopa Troopa Car."),
        3748075486 => {
            Some("Reach the goal after defeating (n) Piranha Plant(s)/Jumping Piranha Plant(s).")
        }
        3797704544 => Some("Reach the goal after defeating (n) Dry Bones."),
        3824561269 => Some("Reach the goal after defeating (n) Stingby/Stingbies."),
        3833342952 => Some("Reach the goal after defeating (n) Piranha Creeper(s)."),
        3842179831 => Some("Reach the goal after defeating (n) Fire Piranha Plant(s)."),
        3874680510 => Some("Reach the goal after breaking (n) Crates(s)."),
        3974581191 => Some("Reach the goal after defeating (n) Ludwig(s)."),
        3977257962 => Some("Reach the goal as Super Mario."),
        4042480826 => Some("Reach the goal after defeating (n) Skipsqueak(s)."),
        4116396131 => Some("Reach the goal after grabbing (n) Coin(s)."),
        4117878280 => Some("Reach the goal after defeating (n) Magikoopa(s)."),
        4122555074 => Some("Reach the goal after grabbing (n) 30-Coin(s)."),
        4153835197 => Some("Reach the goal as Balloon Mario."),
        4172105156 => Some("Reach the goal while wearing a Red POW Box."),
        4209535561 => Some("Reach the Goal while riding Yoshi."),
        4269094462 => Some("Reach the goal after defeating (n) Spike Top(s)."),
        4293354249 => Some("Reach the goal after defeating (n) Banzai Bill(s)."),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatted_level_id_formats_correctly() {
        assert_eq!(Smm2Level::formatted_level_id("abc123DEF"), "ABC-123-DEF")
    }
}
