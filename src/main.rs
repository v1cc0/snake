mod config;
mod proxy;
mod service;
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
use test::{run_test, TestMode as TestModeEnum};
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;
use update::check_and_update;
use axum_server::tls_rustls::RustlsConfig;

// --- CLI Structure ---
const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO_OWNER: &str = "v1cc0";
const REPO_NAME: &str = "snake";

#[derive(Parser)]
#[command(name = "snake")]
#[command(version = VERSION)]
#[command(about = "Snake - the API proxy", long_about = None)]
struct Cli {
    /// Path to config file (default: config.toml)
    #[arg(short, long, global = true, default_value = "config.toml")]
    config: String,

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
    Test {
        #[command(subcommand)]
        mode: Option<TestMode>,
    },
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage systemd service
    Service {
        #[command(subcommand)]
        action: ServiceAction,
    },
}

#[derive(Subcommand)]
enum TestMode {
    /// Test all (gateways and providers)
    All,
    /// Test gateway rotation only
    Gateway,
    /// Test specific provider
    Provider {
        /// Provider name (e.g., openai, google-ai-studio, groq)
        name: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Check if config file is valid and meets minimum requirements
    Check {
        /// Path to config file to check (overrides --config)
        path: Option<String>,
    },
}

#[derive(Subcommand)]
enum ServiceAction {
    /// Install and start the systemd service
    Start,
    /// Stop and uninstall the systemd service
    Stop,
}

#[tokio::main]
async fn main() {
    // Install ring crypto provider for rustls BEFORE any TLS operations
    // This prevents the "Could not automatically determine CryptoProvider" panic
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // Initialize tracing (for logging)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set tracing subscriber");

    info!("Starting Snake - the API proxy v{}", VERSION);

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
        Some(Commands::Test { mode }) => {
            let test_mode = match mode.unwrap_or(TestMode::All) {
                TestMode::All => TestModeEnum::All,
                TestMode::Gateway => TestModeEnum::Gateway,
                TestMode::Provider { name } => TestModeEnum::Provider(name),
            };
            if let Err(e) = run_test(&cli.config, test_mode).await {
                error!("Test failed: {}", e);
                eprintln!("\n❌ Test failed: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Config { action }) => {
            match action {
                ConfigAction::Check { path } => {
                    let config_path = path.as_ref().unwrap_or(&cli.config);
                    if let Err(e) = check_config(config_path) {
                        error!("Config check failed: {}", e);
                        eprintln!("\n❌ Config check failed: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            return;
        }
        Some(Commands::Service { action }) => {
            let result = match action {
                ServiceAction::Start => service::install_service(),
                ServiceAction::Stop => service::uninstall_service(),
            };
            if let Err(e) = result {
                error!("Service command failed: {}", e);
                eprintln!("\n❌ Service command failed: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Serve) | None => {
            // Continue to serve mode (default)
        }
    }

    // Load configuration from specified path
    let config = match Config::from_toml(&cli.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Configuration error: {}", e);
            eprintln!("Configuration error: {}", e);
            return;
        }
    };

    info!(
        "Loaded {} gateway(s) for round-robin rotation",
        config.gateways.len()
    );

    // Display server mode and endpoints
    if config.https_server {
        info!("Server mode: HTTPS (port {})", config.https_port);
        info!("  TLS Certificate: {}", config.tls_cert_path);
        info!("  TLS Private Key: {}", config.tls_key_path);
        info!(
            "Public endpoint: https://0.0.0.0:{}/v1/chat/completions",
            config.https_port
        );
    } else {
        info!("Server mode: HTTP (port {})", config.http_port);
        info!(
            "Local endpoint: http://0.0.0.0:{}/v1/chat/completions",
            config.http_port
        );
    }

    // Test network connectivity to Cloudflare AI Gateway before starting server
    info!("Testing network connectivity to gateway.ai.cloudflare.com...");
    let test_client = Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .expect("Failed to build HTTP client");

    let test_url = "https://gateway.ai.cloudflare.com";
    match test_client.head(test_url).send().await {
        Ok(response) => {
            if response.status().is_success() || response.status().is_redirection() {
                info!("✓ Network connectivity test passed (status: {})", response.status());
            } else {
                error!("Network connectivity test failed: HTTP {}", response.status());
                eprintln!("\n❌ Error: Cannot reach Cloudflare AI Gateway");
                eprintln!("   URL: {}", test_url);
                eprintln!("   Status: {}", response.status());
                eprintln!("\nPlease check:");
                eprintln!("  1. Your internet connection");
                eprintln!("  2. Firewall settings");
                eprintln!("  3. DNS resolution for gateway.ai.cloudflare.com");
                std::process::exit(1);
            }
        }
        Err(e) => {
            error!("Network connectivity test failed: {}", e);
            eprintln!("\n❌ Error: Cannot reach Cloudflare AI Gateway");
            eprintln!("   URL: {}", test_url);
            eprintln!("   Error: {}", e);
            eprintln!("\nPlease check:");
            eprintln!("  1. Your internet connection");
            eprintln!("  2. Firewall settings");
            eprintln!("  3. DNS resolution for gateway.ai.cloudflare.com");
            eprintln!("  4. Proxy settings (if applicable)");
            std::process::exit(1);
        }
    }

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

    // Start server based on HTTPS configuration
    if config.https_server {
        // HTTPS mode
        info!("Starting HTTPS server on 0.0.0.0:{}", config.https_port);

        // Load TLS configuration
        let tls_config = match load_tls_config(&config.tls_cert_path, &config.tls_key_path).await {
            Ok(cfg) => cfg,
            Err(e) => {
                error!("Failed to load TLS configuration: {}", e);
                eprintln!("\n❌ Error: Failed to load TLS configuration");
                eprintln!("   {}", e);
                eprintln!("\nPlease check:");
                eprintln!("  1. Certificate file exists: {}", config.tls_cert_path);
                eprintln!("  2. Private key file exists: {}", config.tls_key_path);
                eprintln!("  3. Files are readable and in correct PEM format");
                std::process::exit(1);
            }
        };

        info!("✓ TLS configuration loaded successfully");
        info!("🚀 HTTPS proxy server ready on port {}", config.https_port);

        if let Err(e) = axum_server::bind_rustls(addr, tls_config)
            .serve(app.into_make_service())
            .await
        {
            error!("HTTPS server error: {}", e);
        }
    } else {
        // HTTP mode
        info!("Starting HTTP server on 0.0.0.0:{}", config.http_port);

        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(listener) => listener,
            Err(e) => {
                error!("Failed to bind to address {}: {}", addr, e);
                return;
            }
        };

        info!("🚀 HTTP proxy server ready on port {}", config.http_port);

        if let Err(e) = axum::serve(listener, app).await {
            error!("Server error: {}", e);
        }
    }
}

/// Check if config file is valid and meets minimum requirements
fn check_config(config_path: &str) -> Result<(), Box<dyn std::error::Error>> {
    info!("Checking configuration file: {}", config_path);

    // Check if file exists
    let path = std::path::Path::new(config_path);
    if !path.exists() {
        return Err(format!("Config file not found: {}", config_path).into());
    }

    println!("✓ Config file exists: {}", config_path);

    // Try to load config
    let config = Config::from_toml(config_path)?;

    // Validate minimum requirements
    println!("\n📋 Configuration Summary:");
    println!("  ├─ Host Port: {}", config.listen_addr.split(':').last().unwrap_or("unknown"));
    println!("  ├─ Gateways: {}", config.gateways.len());

    if config.gateways.is_empty() {
        return Err("At least one gateway must be configured".into());
    }

    for (idx, gateway) in config.gateways.iter().enumerate() {
        println!("  │   └─ Gateway {}: {}/{}",
            idx + 1,
            gateway.account_id.chars().take(8).collect::<String>() + "...",
            gateway.gateway_id
        );
    }

    // Count configured providers
    let mut provider_count = 0;
    println!("  └─ Providers:");
    for (name, provider) in &config.providers {
        if !provider.api_keys.is_empty() {
            provider_count += 1;
            println!("      ├─ {}: {} key(s)", name, provider.api_keys.len());
        }
    }

    if provider_count == 0 {
        println!("\n⚠️  Warning: No provider API keys configured");
        println!("   The proxy will work but will use client-provided API keys only");
    }

    println!("\n✅ Configuration is valid and ready to use");
    println!("\nMinimum requirements met:");
    println!("  ✓ At least 1 gateway configured ({} found)", config.gateways.len());
    println!("  ✓ Valid TOML syntax");
    println!("  {} Provider API keys configured", provider_count);

    Ok(())
}

/// Load TLS configuration from certificate and private key files
async fn load_tls_config(cert_path: &str, key_path: &str) -> Result<RustlsConfig, Box<dyn std::error::Error>> {
    // Verify files exist before attempting to load
    if !std::path::Path::new(cert_path).exists() {
        return Err(format!("Certificate file not found: {}", cert_path).into());
    }
    if !std::path::Path::new(key_path).exists() {
        return Err(format!("Private key file not found: {}", key_path).into());
    }

    // Use RustlsConfig::from_pem_file which handles certificate and key loading
    RustlsConfig::from_pem_file(cert_path, key_path)
        .await
        .map_err(|e| format!("Failed to load TLS configuration: {}", e).into())
}
