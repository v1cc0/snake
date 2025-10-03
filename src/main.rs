mod config;
mod proxy;
mod stream;
mod test;
mod update;

use axum::Router;
use clap::{Parser, Subcommand};
use config::Config;
use proxy::{AppState, proxy_handler};
use reqwest::Client;
use std::env;
use std::net::SocketAddr;
use test::run_test;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;
use update::check_and_update;

// --- CLI Structure ---
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO_OWNER: &str = "v1cc0";
const REPO_NAME: &str = "snake";

#[derive(Parser)]
#[command(name = "snake")]
#[command(version = VERSION)]
#[command(about = "Cloudflare AI Gateway Proxy", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Check for updates and upgrade to the latest version
    Update {
        /// Skip confirmation prompt and update directly
        #[arg(short, long)]
        yes: bool,
        /// GitHub personal access token for downloading releases (optional)
        #[arg(short, long)]
        token: Option<String>,
    },
    /// Start the proxy server (default if no command specified)
    Serve,
    /// Test the proxy configuration and connection
    Test,
}

#[tokio::main]
async fn main() {
    // Initialize tracing (for logging)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    info!("Starting Cloudflare AI Gateway Proxy v{}", VERSION);

    // Parse CLI arguments
    let cli = Cli::parse();

    // Handle commands
    match cli.command {
        Some(Commands::Update { yes, token }) => {
            if let Err(e) = check_and_update(VERSION, REPO_OWNER, REPO_NAME, yes, token).await {
                error!("Update failed: {}", e);
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Test) => {
            if let Err(e) = run_test().await {
                error!("Test failed: {}", e);
                eprintln!("\n❌ Test failed: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Serve) | None => {
            // Continue to serve mode (default)
        }
    }

    // Load configuration
    let config = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Configuration error: {}", e);
            eprintln!("Configuration error: {}", e);
            return;
        }
    };

    info!(
        "Cloudflare Base AI Gateway URL: {}",
        config.cf_base_gateway_url
    );
    info!(
        "Local endpoint: http://{}/v1/chat/completions",
        config.listen_addr
    );

    // Create a single, shared reqwest client for connection pooling and performance.
    let client = Client::new();
    let app_state = AppState {
        client,
        config: config.clone(),
    };

    // Define the application routes.
    let app = Router::new()
        .route("/{*path}", axum::routing::any(proxy_handler))
        .with_state(app_state);

    // Parse the listening address
    let addr: SocketAddr = match config.listen_addr.parse() {
        Ok(addr) => addr,
        Err(_) => {
            error!("Failed to parse listen address: {}", config.listen_addr);
            return;
        }
    };
    info!("Gateway proxy listening on {}", addr);

    // Start the server
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind to address {}: {}", addr, e);
            return;
        }
    };
    if let Err(e) = axum::serve(listener, app).await {
        error!("Server error: {}", e);
    }
}
