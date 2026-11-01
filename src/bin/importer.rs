use clap::Parser;
use smm_zerop::{
    components::{settings::Settings, smm2_importer},
    get_db_pool, init_tracing,
};
use tracing::info;

fn main() -> anyhow::Result<()> {
    let settings = Settings::parse();

    let mut rt = tokio::runtime::Builder::new_multi_thread();
    if let Some(threads) = settings.threads {
        rt.worker_threads(threads);
    }

    rt.enable_all()
        .build()?
        .block_on(async { run(settings).await })
}

async fn run(settings: Settings) -> anyhow::Result<()> {
    init_tracing(&settings);

    info!("connecting to own database...");
    let own_db = get_db_pool(settings.database_url).await?;

    smm2_importer::run(own_db).await?;
    Ok(())
}
