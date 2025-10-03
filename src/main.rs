use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode, header},
    response::{IntoResponse, Response},
};
use clap::{Parser, Subcommand};
use http_body_util::BodyExt;
use reqwest::Client;
use serde_json::{Value, json};
use std::env;
use std::net::SocketAddr;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

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
    },
    /// Start the proxy server (default if no command specified)
    Serve,
}

// --- Custom Error Type ---
enum ProxyError {
    BadRequest(String),
    BadGateway(String),
}

impl IntoResponse for ProxyError {
    fn into_response(self) -> Response {
        let (status, error_message) = match self {
            ProxyError::BadRequest(msg) => {
                error!("Bad Request: {}", msg);
                (StatusCode::BAD_REQUEST, msg)
            }
            ProxyError::BadGateway(msg) => {
                error!("Bad Gateway: {}", msg);
                (StatusCode::BAD_GATEWAY, msg)
            }
        };
        (status, error_message).into_response()
    }
}

// --- CONFIGURATION ---
// Structure to hold runtime configuration loaded from environment variables
#[derive(Clone)]
struct Config {
    // Cloudflare AI Gateway a base URL for the, not including the compatibility mode path
    cf_base_gateway_url: String,
    // The path segment for the OpenAI compatibility mode
    openai_compat_path: String,
    listen_addr: String,
}

impl Config {
    fn from_env() -> Result<Self, String> {
        dotenvy::dotenv().ok();
        info!("Environment variables loaded from .env file (if present).");

        let host_port = env::var("HOST_PORT").unwrap_or_else(|_| {
            info!("HOST_PORT environment variable not set, defaulting to 3000.");
            "3000".to_string()
        });
        let listen_addr = format!("0.0.0.0:{}", host_port);

        let account_id = env::var("ACCOUNT_ID")
            .map_err(|_| "ACCOUNT_ID environment variable must be set".to_string())?;
        let gateway_id = env::var("GATEWAY_ID")
            .map_err(|_| "GATEWAY_ID environment variable must be set".to_string())?;

        let cf_base_gateway_url = format!(
            "https://gateway.ai.cloudflare.com/v1/{}/{}",
            account_id, gateway_id
        );
        let openai_compat_path = "/compat".to_string();

        Ok(Self {
            cf_base_gateway_url,
            openai_compat_path,
            listen_addr,
        })
    }
}

// The application state, which holds the shared reqwest client and config.
#[derive(Clone)]
struct AppState {
    client: Client,
    config: Config,
}

// --- Update Functionality ---
async fn check_and_update(skip_confirm: bool) -> Result<(), Box<dyn std::error::Error>> {
    info!("Current version: {}", VERSION);
    info!("Checking for updates from GitHub repository: {}/{}", REPO_OWNER, REPO_NAME);

    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("snake")
        .show_download_progress(true)
        .current_version(VERSION)
        .build()?;

    let latest_release = status.get_latest_release()?;
    let latest_version = latest_release.version.trim_start_matches('v');

    info!("Latest version available: {}", latest_version);

    // Compare versions
    let current = semver::Version::parse(VERSION)?;
    let latest = semver::Version::parse(latest_version)?;

    if current >= latest {
        info!("You are already running the latest version!");
        return Ok(());
    }

    info!("New version available: {} -> {}", VERSION, latest_version);

    // Confirm update if not skipped
    if !skip_confirm {
        println!("\nA new version is available: {} -> {}", VERSION, latest_version);
        println!("Do you want to update? (y/N): ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            info!("Update cancelled by user");
            return Ok(());
        }
    }

    info!("Downloading and installing update...");
    let status = self_update::backends::github::Update::configure()
        .repo_owner(REPO_OWNER)
        .repo_name(REPO_NAME)
        .bin_name("snake")
        .show_download_progress(true)
        .current_version(VERSION)
        .build()?
        .update()?;

    info!("Successfully updated to version: {}", status.version());
    println!("\n✓ Update successful! New version: {}", status.version());
    println!("Please restart the application to use the new version.");

    Ok(())
}

#[tokio::main]
async fn main() {
    // Initialize tracing (for logging)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        eprintln!("setting default subscriber failed");
        return;
    }

    // Parse CLI arguments
    let cli = Cli::parse();

    // Handle commands
    match cli.command {
        Some(Commands::Update { yes }) => {
            if let Err(e) = check_and_update(yes).await {
                error!("Update failed: {}", e);
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Serve) | None => {
            // Continue to start the server (default behavior)
        }
    }

    // Load configuration
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            return;
        }
    };

    info!(
        "Cloudflare Base AI Gateway URL: {}",
        config.cf_base_gateway_url
    );
    info!("OpenAI Compatibility Path: {}", config.openai_compat_path);

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

/// The main handler function that proxies requests.
async fn proxy_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response, ProxyError> {
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let headers = parts.headers;
    let path_query = parts
        .uri
        .path_and_query()
        .map(|v| v.as_str())
        .unwrap_or(parts.uri.path());

    // Construct the full target URL
    let target_url = format!(
        "{}{}{}",
        state.config.cf_base_gateway_url, state.config.openai_compat_path, path_query
    );

    info!("Forwarding request to: {} {}", method, target_url);

    // Read the request body
    let full_body = body
        .collect()
        .await
        .map_err(|e| ProxyError::BadRequest(format!("Failed to read request body: {}", e)))?;
    let body_bytes = full_body.to_bytes();

    // Try to parse the body as JSON and check for stream parameter
    let (modified_body, was_stream_request) = if let Ok(mut json_body) = serde_json::from_slice::<Value>(&body_bytes) {
        let was_stream = json_body.get("stream").and_then(|v| v.as_bool()).unwrap_or(false);

        if was_stream {
            info!("Detected stream request, converting to non-stream for Cloudflare");
            json_body["stream"] = json!(false);
            let modified = serde_json::to_vec(&json_body)
                .map_err(|e| ProxyError::BadRequest(format!("Failed to serialize modified body: {}", e)))?;
            (modified, true)
        } else {
            (body_bytes.to_vec(), false)
        }
    } else {
        // Not a JSON body or parsing failed, use as-is
        (body_bytes.to_vec(), false)
    };

    // Send request to Cloudflare
    let client_request = state.client.request(method, &target_url).headers(headers);
    let response = client_request
        .body(modified_body)
        .send()
        .await
        .map_err(|e| {
            ProxyError::BadGateway(format!("Failed to forward request to target: {}", e))
        })?;

    let status = response.status();
    let response_headers = response.headers().clone();

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ProxyError::BadGateway(format!("Failed to read response body: {}", e)))?;

    // If the original request wanted streaming, convert the response to SSE format
    if was_stream_request {
        info!("Converting response to SSE stream format");
        return Ok(convert_to_sse_stream(status, bytes));
    }

    // Otherwise, return the response as-is
    let mut axum_res = Response::new(Body::empty());
    *axum_res.status_mut() = status;
    *axum_res.headers_mut() = response_headers;
    *axum_res.body_mut() = Body::from(bytes);

    Ok(axum_res)
}

/// Converts a complete response to SSE (Server-Sent Events) stream format
fn convert_to_sse_stream(status: StatusCode, response_bytes: bytes::Bytes) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(100);

    tokio::spawn(async move {
        // Parse the response JSON
        if let Ok(json_response) = serde_json::from_slice::<Value>(&response_bytes) {
            // Check if it's a chat completion response
            if let Some(choices) = json_response.get("choices").and_then(|v| v.as_array()) {
                // Send each choice as a separate SSE chunk
                for choice in choices {
                    let mut stream_chunk = json!({
                        "choices": [choice],
                        "created": json_response.get("created").cloned().unwrap_or(json!(0)),
                        "id": json_response.get("id").cloned().unwrap_or(json!("unknown")),
                        "model": json_response.get("model").cloned().unwrap_or(json!("unknown")),
                        "object": "chat.completion.chunk"
                    });

                    // Add usage info if this is the last chunk
                    if let Some(usage) = json_response.get("usage") {
                        stream_chunk["usage"] = usage.clone();
                    }

                    let sse_data = format!("data: {}\n\n", serde_json::to_string(&stream_chunk).unwrap_or_default());
                    if tx.send(Ok(sse_data)).await.is_err() {
                        break;
                    }
                }
            } else {
                // Not a standard chat completion, send the whole response as one chunk
                let sse_data = format!("data: {}\n\n", serde_json::to_string(&json_response).unwrap_or_default());
                let _ = tx.send(Ok(sse_data)).await;
            }
        } else {
            // Failed to parse JSON, send raw data
            if let Ok(text) = String::from_utf8(response_bytes.to_vec()) {
                let sse_data = format!("data: {}\n\n", text);
                let _ = tx.send(Ok(sse_data)).await;
            }
        }

        // Send the [DONE] marker
        let _ = tx.send(Ok("data: [DONE]\n\n".to_string())).await;
    });

    let stream = ReceiverStream::new(rx);
    let body = Body::from_stream(stream);

    let mut response = Response::new(body);
    *response.status_mut() = status;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        "text/event-stream".parse().unwrap(),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-cache".parse().unwrap(),
    );
    response.headers_mut().insert(
        header::CONNECTION,
        "keep-alive".parse().unwrap(),
    );

    response
}
