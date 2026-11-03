//! Roostbase server: serves the web UI in a browser (local use and Docker).

use clap::Parser;
use roostbase::{default_data_dir, router, Options};
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "roostbase-server", version, about = "Roostbase: database explorer and query tool (web server)")]
struct Cli {
    /// Database URL to open on start (postgres://, mysql://, sqlite://).
    #[arg(long, env = "DATABASE_URL")]
    url: Option<String>,
    /// Host to bind.
    #[arg(long, env = "STUDIO_HOST", default_value = "127.0.0.1")]
    host: String,
    /// Port to bind.
    #[arg(long, short, env = "STUDIO_PORT", default_value_t = 4983)]
    port: u16,
    /// Directory for saved connections.
    #[arg(long, env = "STUDIO_DATA_DIR")]
    data_dir: Option<PathBuf>,
    /// Do not open the browser on start.
    #[arg(long, env = "STUDIO_NO_OPEN")]
    no_open: bool,
    /// Block all data changes.
    #[arg(long, env = "STUDIO_READ_ONLY")]
    read_only: bool,
    /// Basic auth user (auth is enabled when a password is set).
    #[arg(long, env = "STUDIO_USER", default_value = "admin")]
    user: String,
    /// Basic auth password.
    #[arg(long, env = "STUDIO_PASSWORD")]
    password: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "roostbase=info,warn".into()),
        )
        .init();
    let cli = Cli::parse();
    let opts = Options {
        url: cli.url.clone(),
        data_dir: cli.data_dir.clone().unwrap_or_else(default_data_dir),
        read_only: cli.read_only,
        basic_auth: cli.password.clone().map(|p| (cli.user.clone(), p)),
        app_token: None,
    };
    let (app, store) = router(&opts);
    let addr: SocketAddr = format!("{}:{}", cli.host, cli.port).parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let shown_host = if cli.host == "0.0.0.0" { "localhost" } else { cli.host.as_str() };
    let url = format!("http://{shown_host}:{}", cli.port);
    println!("\n  Roostbase is running at {url}");
    println!("  Saved connections: {}\n", store.display());
    if !cli.no_open {
        let _ = open::that_detached(&url);
    }
    axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}
