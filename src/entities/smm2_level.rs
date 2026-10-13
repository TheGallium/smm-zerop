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

impl ClearConditionGroup {
    fn id_list(&self) -> Option<Vec<i64>> {
        match self {
            Self::None => None,
            Self::NoJumping => Some(vec![1]),
            Self::NoDamage => Some(vec![4]),
            Self::DefeatingEnemies => Some(vec![
                2, 3, 9, 11, 14, 15, 17, 18, 19, 20, 22, 24, 25, 26, 27, 29, 30, 32, 33, 34, 36,
                39, 40, 42, 43, 44, 45, 46, 50, 51, 52, 54, 55, 61, 65, 67, 68, 69, 70, 71, 72, 77,
                78, 79, 80, 81, 83, 85, 87, 92, 93,
            ]),
            Self::PowerupFinish => Some(vec![
                5, 6, 7, 8, 10, 12, 13, 21, 28, 41, 47, 48, 49, 53, 57, 58, 59, 60, 62, 63, 64, 73,
                74, 75, 76, 84, 89, 90, 91,
            ]),
            Self::HoldingActivating => Some(vec![16, 23, 31, 37, 38]),
            Self::Collecting => Some(vec![35, 56, 66, 82, 86, 88]),
        }
    }
}

fn clear_condition_label(id: i64) -> Option<&'static str> {
    match id {
        0 => None,
        1 => Some("Reach the goal without landing after leaving the ground."),
        2 => Some("Reach the goal after defeating (n) Mechakoopa(s)."),
        3 => Some("Reach the goal after defeating (n) Cheep Cheep(s)."),
        4 => Some("Reach the goal without taking damage."),
        5 => Some("Reach the goal as Boomerang Mario."),
        6 => Some("Reach the goal while wearing a Shoe."),
        7 => Some("Reach the goal as Fire Mario."),
        8 => Some("Reach the goal as Frog Mario."),
        9 => Some("Reach the goal after defeating (n) Larry(s)."),
        10 => Some("Reach the goal as Raccoon Mario."),
        11 => Some("Reach the goal after defeating (n) Blooper(s)."),
        12 => Some("Reach the goal as Propeller Mario."),
        13 => Some("Reach the goal while wearing a Propeller Box."),
        14 => Some("Reach the goal after defeating (n) Spike(s)."),
        15 => Some("Reach the goal after defeating (n) Boom Boom(s)."),
        16 => Some("Reach the goal while holding a Koopa Shell."),
        17 => Some("Reach the goal after defeating (n) Porcupuffer(s)."),
        18 => Some("Reach the goal after defeating (n) Charvaargh(s)."),
        19 => Some("Reach the goal after defeating (n) Bullet Bill(s)."),
        20 => Some("Reach the goal after defeating (n) Bully/Bullies."),
        21 => Some("Reach the goal while wearing a Goomba Mask."),
        22 => Some("Reach the goal after defeating (n) Hop-Chops."),
        23 => Some(
            "Reach the goal while holding a Red POW Block. OR Reach the goal after activating (n) Red POW Block(s).",
        ),
        24 => Some("Reach the goal after defeating (n) Bob-omb(s)."),
        25 => Some("Reach the goal after defeating (n) Spiny/Spinies."),
        26 => Some("Reach the goal after defeating (n) Bowser(s)/Meowser(s)."),
        27 => Some("Reach the goal after defeating (n) Ant Trooper(s)."),
        28 => Some("Reach the goal on a Lakitu's Cloud."),
        29 => Some("Reach the goal after defeating (n) Boo(s)."),
        30 => Some("Reach the goal after defeating (n) Roy(s)."),
        31 => Some("Reach the goal while holding a Trampoline."),
        32 => Some("Reach the goal after defeating (n) Morton(s)."),
        33 => Some("Reach the goal after defeating (n) Fish Bone(s)."),
        34 => Some("Reach the goal after defeating (n) Monty Mole(s)."),
        35 => Some("Reach the goal after picking up (n) 1-Up Mushroom(s)."),
        36 => Some("Reach the goal after defeating (n) Hammer Bro(s)."),
        37 => Some(
            "Reach the goal after hitting (n) P Switch(es). OR Reach the goal while holding a P Switch.",
        ),
        38 => Some(
            "Reach the goal after activating (n) POW Block(s). OR Reach the goal while holding a POW Block.",
        ),
        39 => Some("Reach the goal after defeating (n) Angry Sun(s)."),
        40 => Some("Reach the goal after defeating (n) Pokey(s)."),
        41 => Some("Reach the goal as Superball Mario."),
        42 => Some("Reach the goal after defeating (n) Pom Pom(s)."),
        43 => Some("Reach the goal after defeating (n) Peepa(s)."),
        44 => Some("Reach the goal after defeating (n) Lakitu(s)."),
        45 => Some("Reach the goal after defeating (n) Lemmy(s)."),
        46 => Some("Reach the goal after defeating (n) Lava Bubble(s)."),
        47 => Some("Reach the goal while wearing a Bullet Bill Mask."),
        48 => Some("Reach the goal as Big Mario."),
        49 => Some("Reach the goal as Cat Mario."),
        50 => Some("Reach the goal after defeating (n) Goomba(s)/Galoomba(s)."),
        51 => Some("Reach the goal after defeating (n) Thwomp(s)."),
        52 => Some("Reach the goal after defeating (n) Iggy(s)."),
        53 => Some("Reach the goal while wearing a Dry Bones Shell."),
        54 => Some("Reach the goal after defeating (n) Sledge Bro(s)."),
        55 => Some("Reach the goal after defeating (n) Rocky Wrench(es)."),
        56 => Some("Reach the goal after grabbing (n) 50-Coin(s)."),
        57 => Some("Reach the goal as Flying Squirrel Mario."),
        58 => Some("Reach the goal as Buzzy Mario."),
        59 => Some("Reach the goal as Builder Mario."),
        60 => Some("Reach the goal as Cape Mario."),
        61 => Some("Reach the goal after defeating (n) Wendy(s)."),
        62 => Some("Reach the goal while wearing a Cannon Box."),
        63 => Some("Reach the goal as Link."),
        64 => Some("Reach the goal while you have Super Star invincibility."),
        65 => Some("Reach the goal after defeating (n) Goombrat(s)/Goombud(s)."),
        66 => Some("Reach the goal after grabbing (n) 10-Coin(s)."),
        67 => Some("Reach the goal after defeating (n) Buzzy Beetle(s)."),
        68 => Some("Reach the goal after defeating (n) Bowser Jr.(s)."),
        69 => Some("Reach the goal after defeating (n) Koopa Troopa(s)."),
        70 => Some("Reach the goal after defeating (n) Chain Chomp(s)."),
        71 => Some("Reach the goal after defeating (n) Muncher(s)."),
        72 => Some("Reach the goal after defeating (n) Wiggler(s)."),
        73 => Some("Reach the goal as SMB2 Mario."),
        74 => Some("Reach the goal in a Koopa Clown Car/Junior Clown Car."),
        75 => Some("Reach the goal as Spiny Mario."),
        76 => Some("Reach the goal in a Koopa Troopa Car."),
        77 => Some("Reach the goal after defeating (n) Piranha Plant(s)/Jumping Piranha Plant(s)."),
        78 => Some("Reach the goal after defeating (n) Dry Bones."),
        79 => Some("Reach the goal after defeating (n) Stingby/Stingbies."),
        80 => Some("Reach the goal after defeating (n) Piranha Creeper(s)."),
        81 => Some("Reach the goal after defeating (n) Fire Piranha Plant(s)."),
        82 => Some("Reach the goal after breaking (n) Crates(s)."),
        83 => Some("Reach the goal after defeating (n) Ludwig(s)."),
        84 => Some("Reach the goal as Super Mario."),
        85 => Some("Reach the goal after defeating (n) Skipsqueak(s)."),
        86 => Some("Reach the goal after grabbing (n) Coin(s)."),
        87 => Some("Reach the goal after defeating (n) Magikoopa(s)."),
        88 => Some("Reach the goal after grabbing (n) 30-Coin(s)."),
        89 => Some("Reach the goal as Balloon Mario."),
        90 => Some("Reach the goal while wearing a Red POW Box."),
        91 => Some("Reach the Goal while riding Yoshi."),
        92 => Some("Reach the goal after defeating (n) Spike Top(s)."),
        93 => Some("Reach the goal after defeating (n) Banzai Bill(s)."),
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
