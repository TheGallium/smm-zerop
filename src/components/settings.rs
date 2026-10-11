use std::net::SocketAddr;

use sqlx::postgres::PgConnectOptions;

/// Specifies the log's output format
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LogFormat {
    Text,
    TextColor,
    Json,
}

/// Specifies how much log output the app generates
#[derive(clap::ValueEnum, Clone, Debug)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn tracing_level(&self) -> tracing::Level {
        use tracing::Level;

        match self {
            LogLevel::Trace => Level::TRACE,
            LogLevel::Debug => Level::DEBUG,
            LogLevel::Info => Level::INFO,
            LogLevel::Warn => Level::WARN,
            LogLevel::Error => Level::ERROR,
        }
    }
}

#[derive(Clone, Debug, clap::Parser)]
#[clap(about, version, propagate_version = true)]
pub struct Settings {
    /// The database URL to connect to. Needs to be a valid PostgreSQL
    /// connection URL, like `postgres://postgres@127.0.0.1/smm_zerop`
    #[clap(long, env = "DATABASE_URL")]
    pub database_url: PgConnectOptions,

    /// Discord Webhook Bot: ID
    #[clap(long, env = "DISCORD_WEBHOOK_ID")]
    pub discord_webhook_id: String,

    /// Discord Webhook Bot: Token
    #[clap(long, env = "DISCORD_WEBHOOK_TOKEN")]
    pub discord_webhook_token: String,

    /// Discord Webhook Bot: Mention Tarket
    #[clap(long, env = "DISCORD_WEBHOOK_MENTION_TARGET")]
    pub discord_webhook_mention_target: String,

    /// The Socket address the server should listen on
    #[clap(long, env = "LISTEN", default_value = "[::1]:8081")]
    pub listen: SocketAddr,

    /// Defines how the log output will be formatted
    #[clap(value_enum, long, env = "LOG_FORMAT", default_value_t = LogFormat::TextColor)]
    pub log_format: LogFormat,

    /// Defines how noisy the server should be
    #[clap(value_enum, long, env = "LOG_LEVEL", default_value_t = LogLevel::Info)]
    pub log_level: LogLevel,

    /// The public URL, including protocol and port, where this application is
    /// reachable to users, like `https://app.exmaple.com`
    #[clap(long, env = "PUBLIC_URL")]
    pub public_url: reqwest::Url,

    /// Limits the number of threads used - defaults to the number of CPU cores
    #[clap(long, env = "THREADS")]
    pub threads: Option<usize>,

    /// Connection String for the Upstream levels database (MSSQL)
    #[clap(long, env = "UPSTREAM_DB_CONNSTRING")]
    pub upstream_db_connstring: Option<String>,
}
