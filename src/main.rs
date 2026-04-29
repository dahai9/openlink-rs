use std::sync::Arc;

use clap::Parser;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

use openlink::config::{self, Config};
use openlink::executor::Executor;
use openlink::server::{self, AppState};

#[derive(Parser)]
#[command(name = "openlink", version = "1.0.0")]
struct Cli {
    /// Working directory (sandbox root)
    #[arg(long, default_value = ".")]
    dir: std::path::PathBuf,

    /// Listen port
    #[arg(long, default_value = "39527")]
    port: u16,

    /// Tool execution timeout in seconds
    #[arg(long, default_value = "60")]
    timeout: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let root_dir = std::fs::canonicalize(&cli.dir)
        .or_else(|_| std::fs::canonicalize(std::path::absolute(&cli.dir)?))?;
    let token = config::load_or_create_token()?;

    let app_config = Arc::new(Config {
        root_dir,
        port: cli.port,
        timeout: cli.timeout,
        token: token.clone(),
    });

    let executor = Executor::new(app_config.clone());
    let state = Arc::new(AppState {
        config: app_config.clone(),
        executor,
    });

    let app = server::create_router(state).layer(CorsLayer::permissive());

    let addr = format!("127.0.0.1:{}", cli.port);
    tracing::info!("listening on {}", addr);
    println!(
        "请在浏览器中打开以下地址完成认证:\nhttp://127.0.0.1:{}/auth?token={}",
        cli.port, token
    );

    let listener = TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
