use clap::Parser;
use fission_serve::{ServeConfig, run_serve};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fission_serve=info,tower_http=info".into()),
        )
        .init();

    let config = ServeConfig::parse();
    run_serve(config).await
}
