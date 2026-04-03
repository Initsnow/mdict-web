use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use mdict_web_http::{FrontendAssets, HttpState, router};
use mdict_web_service::ReloadableDictionaryService;
use metrics_exporter_prometheus::PrometheusBuilder;
use tracing::info;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
struct Cli {
    #[arg(long, env = "MDICT_WEB_CONFIG", default_value = "mdict-web.toml")]
    config: PathBuf,
    #[arg(long, env = "MDICT_WEB_FRONTEND_DIST", default_value = "frontend/dist")]
    frontend_dist: PathBuf,
    #[arg(long, env = "MDICT_WEB_DISABLE_FRONTEND", default_value_t = false)]
    no_frontend: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init_tracing();

    let cli = Cli::parse();
    let service = ReloadableDictionaryService::load_from_path(&cli.config)
        .await
        .with_context(|| format!("failed to load service from {}", cli.config.display()))?;
    let snapshot = service.snapshot().await;
    let server_config = snapshot.config().server.clone();
    let observability = snapshot.config().observability.clone();
    drop(snapshot);

    let metrics = if observability.metrics_enabled {
        Some(
            PrometheusBuilder::new()
                .install_recorder()
                .context("failed to install prometheus recorder")?,
        )
    } else {
        None
    };

    let frontend = if cli.no_frontend {
        info!("frontend static serving disabled");
        None
    } else {
        let dist = cli.frontend_dist;
        let frontend = FrontendAssets::new(dist.clone());
        match &frontend {
            Some(_) => info!(dist = %dist.display(), "serving frontend static assets from dist"),
            None => info!(dist = %dist.display(), "frontend dist not found; serving API only"),
        }
        frontend
    };

    let state = HttpState::new(service, metrics, frontend);
    let app = router(
        state,
        server_config.request_body_limit_bytes,
        &observability.metrics_path,
    );

    let listener = tokio::net::TcpListener::bind(server_config.bind)
        .await
        .with_context(|| format!("failed to bind {}", server_config.bind))?;
    info!(bind = %server_config.bind, "mdict-web is listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("axum server error")?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};

        match signal(SignalKind::terminate()) {
            Ok(mut stream) => {
                let _ = stream.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
