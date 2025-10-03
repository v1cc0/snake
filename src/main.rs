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
        /// GitHub personal access token for downloading releases (optional)
        #[arg(short, long)]
        token: Option<String>,
    },
    /// Start the proxy server (default if no command specified)
    Serve,
    /// Test the proxy configuration and connection
    Test,
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
    // Cloudflare AI Gateway base URL (e.g., https://gateway.ai.cloudflare.com/v1/{account}/{gateway})
    cf_base_gateway_url: String,
    // The path segment for the provider (e.g., /openai)
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
        let openai_compat_path = "/openai".to_string();

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
async fn check_and_update(skip_confirm: bool, token: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    info!("Current version: {}", VERSION);
    info!("Checking for updates from GitHub repository: {}/{}", REPO_OWNER, REPO_NAME);

    // Use token from CLI argument, or fall back to GITHUB_TOKEN env var
    dotenvy::dotenv().ok();
    let github_token = token.or_else(|| env::var("GITHUB_TOKEN").ok());

    if github_token.is_some() {
        info!("Using GitHub token for API requests");
    }

    let status = if let Some(ref token) = github_token {
        self_update::backends::github::Update::configure()
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(VERSION)
            .auth_token(token)
            .build()?
    } else {
        self_update::backends::github::Update::configure()
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(VERSION)
            .build()?
    };

    let latest_release = status.get_latest_release()?;
    let latest_version = latest_release.version.trim_start_matches('v');

    info!("Latest version available: {}", latest_version);

    // Check if versions are exactly the same
    if VERSION == latest_version {
        info!("You are already running the latest version!");
        return Ok(());
    }

    // Try to parse and compare versions using semver
    let needs_update = match (semver::Version::parse(VERSION), semver::Version::parse(latest_version)) {
        (Ok(current), Ok(latest)) => {
            // Compare major.minor.patch only
            if current.major != latest.major || current.minor != latest.minor || current.patch != latest.patch {
                // Different version numbers - use normal semver comparison
                latest > current
            } else {
                // Same major.minor.patch but different pre-release/build metadata
                // Always offer to update in this case (e.g., 0.0.8 -> 0.0.8-1, 0.0.8-1 -> 0.0.8-2)
                // This handles hotfix releases properly
                true
            }
        }
        _ => {
            // Failed to parse one or both versions - version strings differ, so ask user
            info!("Cannot compare versions using semver, will prompt user");
            true
        }
    };

    if !needs_update {
        info!("Current version ({}) is newer than or equal to latest ({})", VERSION, latest_version);
        return Ok(());
    }

    info!("Version difference detected: {} -> {}", VERSION, latest_version);

    // Confirm update if not skipped
    if !skip_confirm {
        println!("\nA different version is available: {} -> {}", VERSION, latest_version);
        println!("Do you want to update? (y/N): ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            info!("Update cancelled by user");
            return Ok(());
        }
    }

    info!("Downloading and installing update...");
    let status = if let Some(ref token) = github_token {
        self_update::backends::github::Update::configure()
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(VERSION)
            .auth_token(token)
            .build()?
            .update()?
    } else {
        self_update::backends::github::Update::configure()
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(VERSION)
            .build()?
            .update()?
    };

    info!("Successfully updated to version: {}", status.version());
    println!("\n✓ Update successful! New version: {}", status.version());
    println!("Please restart the application to use the new version.");

    Ok(())
}

// --- Test Functionality ---
async fn run_test() -> Result<(), Box<dyn std::error::Error>> {
    info!("Running proxy configuration test...");

    // Check if .env file exists
    let env_path = std::path::Path::new(".env");
    if !env_path.exists() {
        eprintln!("\n❌ Error: .env file not found!");
        eprintln!("Please create a .env file based on .env.template");
        return Err("Missing .env file".into());
    }

    println!("\n✓ .env file found");

    // Load environment variables
    dotenvy::dotenv().ok();

    // Read and display configuration
    println!("\n📋 Current Configuration:");
    println!("  ├─ HOST_PORT: {}", env::var("HOST_PORT").unwrap_or_else(|_| "3000 (default)".to_string()));

    let account_id = env::var("ACCOUNT_ID").unwrap_or_default();
    let gateway_id = env::var("GATEWAY_ID").unwrap_or_default();

    let account_id_display = if account_id.is_empty() {
        "⚠️  NOT SET".to_string()
    } else {
        mask_string(&account_id)
    };
    let gateway_id_display = if gateway_id.is_empty() {
        "⚠️  NOT SET".to_string()
    } else {
        gateway_id.clone()
    };

    println!("  ├─ ACCOUNT_ID: {}", account_id_display);
    println!("  ├─ GATEWAY_ID: {}", gateway_id_display);

    // Check CF_AIG_TOKEN
    let cf_aig_token = env::var("CF_AIG_TOKEN").unwrap_or_default();
    let cf_token_display = if cf_aig_token.is_empty() {
        "⚠️  NOT SET".to_string()
    } else {
        mask_api_key(&cf_aig_token)
    };
    println!("  ├─ CF_AIG_TOKEN: {}", cf_token_display);

    // Check provider API keys
    println!("  └─ Provider API Keys:");
    let mut has_api_key = false;

    for (name, env_key) in [
        ("OpenAI", "OPENAI_API_KEY"),
        ("Claude", "CLAUDE_API_KEY"),
        ("Gemini", "GEMNINI_API_KEY"),
        ("Grok", "GROK_API_KEY"),
        ("Mistral", "MISTRAL_API_KEY"),
        ("Groq", "GROQ_API_KEY"),
    ] {
        let key = env::var(env_key).unwrap_or_default();
        if !key.is_empty() {
            println!("      ├─ {}: {}", name, mask_api_key(&key));
            has_api_key = true;
        } else {
            println!("      ├─ {}: ⚠️  NOT SET", name);
        }
    }

    // Validate required configuration
    if account_id.is_empty() || gateway_id.is_empty() {
        eprintln!("\n❌ Error: Missing required configuration!");
        eprintln!("Please set ACCOUNT_ID and GATEWAY_ID in your .env file");
        return Err("Missing required configuration".into());
    }

    if !has_api_key {
        eprintln!("\n⚠️  Warning: No provider API key configured!");
        eprintln!("Please set at least one API key (e.g., OPENAI_API_KEY, CLAUDE_API_KEY) in your .env file");
        return Err("No API key configured".into());
    }

    println!("\n✓ Configuration validated");

    // Load config for server
    let config = Config::from_env()?;
    let host_port = env::var("HOST_PORT").unwrap_or_else(|_| "3000".to_string());
    let listen_addr = format!("127.0.0.1:{}", host_port);

    // Create HTTP client for testing
    let test_client = Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;

    // Start server in background
    println!("\n🚀 Starting test server on {}...", listen_addr);

    let client = Client::new();
    let app_state = AppState {
        client: client.clone(),
        config: config.clone(),
    };

    let app = Router::new()
        .route("/{*path}", axum::routing::any(proxy_handler))
        .with_state(app_state);

    let addr: SocketAddr = listen_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Spawn server in background
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, app).await
    });

    // Wait for server to start and verify it's listening
    for _ in 0..20 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if tokio::net::TcpStream::connect(&listen_addr).await.is_ok() {
            break;
        }
    }

    println!("✓ Test server started");

    // Determine which model to use for testing
    let test_model = env::var("OPENAI_TEST_MODEL")
        .unwrap_or_else(|_| {
            info!("OPENAI_TEST_MODEL not set, using default: gpt-5-mini");
            "gpt-5-mini".to_string()
        });

    println!("\n📤 Sending test request: \"Do you like snake?\"");
    println!("  └─ Using model: {}", test_model);

    let test_url = format!("http://127.0.0.1:{}/v1/chat/completions", host_port);
    let test_payload = json!({
        "model": test_model,
        "messages": [
            {"role": "user", "content": "Do you like snake?"}
        ]
    });

    // Get CF_AIG_TOKEN and provider API key
    let cf_aig_token = env::var("CF_AIG_TOKEN").unwrap_or_default();
    let provider_key = env::var("OPENAI_API_KEY")
        .or_else(|_| env::var("CLAUDE_API_KEY"))
        .or_else(|_| env::var("GEMNINI_API_KEY"))
        .or_else(|_| env::var("GROK_API_KEY"))
        .or_else(|_| env::var("MISTRAL_API_KEY"))
        .or_else(|_| env::var("GROQ_API_KEY"))
        .unwrap_or_default();

    let mut request = test_client.post(&test_url)
        .header("Content-Type", "application/json")
        .json(&test_payload);

    // Add cf-aig-authorization header if CF_AIG_TOKEN is set
    if !cf_aig_token.is_empty() {
        request = request.header("cf-aig-authorization", format!("Bearer {}", cf_aig_token));
        println!("  ├─ Added cf-aig-authorization header");
    } else {
        println!("  ⚠️  CF_AIG_TOKEN not set, skipping cf-aig-authorization header");
    }

    // Add Authorization header with provider API key
    if !provider_key.is_empty() {
        request = request.header("Authorization", format!("Bearer {}", provider_key));
        println!("  ├─ Added Authorization header");
    } else {
        println!("  ⚠️  No provider API key found, skipping Authorization header");
    }

    match request.send().await
    {
        Ok(response) => {
            let status = response.status();
            println!("\n📥 Response Status: {}", status);

            match response.text().await {
                Ok(body) => {
                    println!("\n📄 Response Body:");
                    // Try to parse as JSON for pretty printing
                    if let Ok(json_body) = serde_json::from_str::<Value>(&body) {
                        println!("{}", serde_json::to_string_pretty(&json_body)?);
                    } else {
                        println!("{}", body);
                    }

                    if status.is_success() {
                        println!("\n✅ Test completed successfully!");
                    } else {
                        println!("\n⚠️  Test completed with non-success status code");
                    }
                }
                Err(e) => {
                    eprintln!("\n❌ Failed to read response body: {}", e);
                    return Err(e.into());
                }
            }
        }
        Err(e) => {
            eprintln!("\n❌ Request failed: {}", e);
            eprintln!("Error code: {}", e);
            return Err(e.into());
        }
    }

    // Abort the server
    server_handle.abort();

    Ok(())
}

fn mask_string(s: &str) -> String {
    if s.len() <= 8 {
        "*".repeat(s.len())
    } else {
        format!("{}...{}", &s[..4], &s[s.len()-4..])
    }
}

fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        "*".repeat(key.len())
    } else {
        format!("{}...{}", &key[..6], &key[key.len()-4..])
    }
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
        Some(Commands::Update { yes, token }) => {
            if let Err(e) = check_and_update(yes, token).await {
                error!("Update failed: {}", e);
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
            return;
        }
        Some(Commands::Test) => {
            if let Err(e) = run_test().await {
                error!("Test failed: {}", e);
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
    info!("Local endpoint: http://{}/v1/chat/completions", config.listen_addr);

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
    // Strip /v1 prefix from path if present (e.g., /v1/chat/completions -> /chat/completions)
    let cleaned_path = path_query.strip_prefix("/v1").unwrap_or(path_query);
    let target_url = format!(
        "{}{}{}",
        state.config.cf_base_gateway_url, state.config.openai_compat_path, cleaned_path
    );

    info!("Forwarding request to: {} {}", method, target_url);

    // Log headers for debugging
    if let Some(cf_aig_auth) = headers.get("cf-aig-authorization") {
        info!("Found cf-aig-authorization header: {:?}", cf_aig_auth);
    } else {
        info!("cf-aig-authorization header not found");
    }
    if let Some(_auth) = headers.get("authorization") {
        info!("Found authorization header");
    } else {
        info!("authorization header not found");
    }

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
    // Filter out hop-by-hop headers and headers that reqwest will set automatically
    let mut filtered_headers = headers.clone();
    filtered_headers.remove("host");  // reqwest will set this based on target URL
    filtered_headers.remove("content-length");  // reqwest will set this based on body size
    filtered_headers.remove("connection");
    filtered_headers.remove("keep-alive");
    filtered_headers.remove("proxy-authenticate");
    filtered_headers.remove("proxy-authorization");
    filtered_headers.remove("te");
    filtered_headers.remove("trailers");
    filtered_headers.remove("transfer-encoding");
    filtered_headers.remove("upgrade");

    info!("Sending request to Cloudflare...");
    if was_stream_request {
        info!("Modified body for non-streaming request, new size: {} bytes", modified_body.len());
    }
    let client_request = state.client.request(method, &target_url).headers(filtered_headers).body(modified_body);
    let response = client_request
        .send()
        .await
        .map_err(|e| {
            error!("Failed to forward request to Cloudflare: {}", e);
            ProxyError::BadGateway(format!("Failed to forward request to target: {}", e))
        })?;

    info!("Received response from Cloudflare, status: {}", response.status());

    let status = response.status();
    let response_headers = response.headers().clone();

    let bytes = response
        .bytes()
        .await
        .map_err(|e| {
            error!("Failed to read response body from Cloudflare: {}", e);
            ProxyError::BadGateway(format!("Failed to read response body: {}", e))
        })?;

    info!("Read response body, {} bytes", bytes.len());

    // If the original request wanted streaming, convert the response to SSE format
    if was_stream_request {
        info!("Converting response to SSE stream format");
        return Ok(convert_to_sse_stream(status, bytes));
    }

    // Otherwise, return the response as-is
    info!("Preparing response to send back to client");

    // Filter out hop-by-hop headers from the response
    let mut filtered_response_headers = response_headers.clone();
    filtered_response_headers.remove("connection");
    filtered_response_headers.remove("keep-alive");
    filtered_response_headers.remove("proxy-authenticate");
    filtered_response_headers.remove("proxy-authorization");
    filtered_response_headers.remove("te");
    filtered_response_headers.remove("trailers");
    filtered_response_headers.remove("transfer-encoding");
    filtered_response_headers.remove("upgrade");

    let mut axum_res = Response::new(Body::from(bytes));
    *axum_res.status_mut() = status;
    *axum_res.headers_mut() = filtered_response_headers;

    info!("Returning response to client");
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
                if let Some(first_choice) = choices.first() {
                    // Extract the full content from the message
                    if let Some(content) = first_choice
                        .get("message")
                        .and_then(|m| m.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        // Get metadata
                        let created = json_response.get("created").cloned().unwrap_or(json!(0));
                        let id = json_response.get("id").cloned().unwrap_or(json!("unknown"));
                        let model = json_response.get("model").cloned().unwrap_or(json!("unknown"));

                        // Split content into words for streaming simulation
                        let words: Vec<&str> = content.split_whitespace().collect();

                        // Send chunks with delays to simulate streaming
                        for (i, word) in words.iter().enumerate() {
                            let word_with_space = if i < words.len() - 1 {
                                format!("{} ", word)
                            } else {
                                word.to_string()
                            };

                            let chunk = json!({
                                "id": id,
                                "object": "chat.completion.chunk",
                                "created": created,
                                "model": model,
                                "choices": [{
                                    "index": 0,
                                    "delta": {
                                        "content": word_with_space
                                    },
                                    "finish_reason": null
                                }]
                            });

                            let sse_data = format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default());
                            if tx.send(Ok(sse_data)).await.is_err() {
                                return;
                            }

                            // Add small delay between chunks
                            tokio::time::sleep(tokio::time::Duration::from_millis(30)).await;
                        }

                        // Send final chunk with finish_reason and usage
                        let mut final_chunk = json!({
                            "id": id,
                            "object": "chat.completion.chunk",
                            "created": created,
                            "model": model,
                            "choices": [{
                                "index": 0,
                                "delta": {},
                                "finish_reason": first_choice.get("finish_reason").cloned().unwrap_or(json!("stop"))
                            }]
                        });

                        // Add usage info if available
                        if let Some(usage) = json_response.get("usage") {
                            final_chunk["usage"] = usage.clone();
                        }

                        let sse_data = format!("data: {}\n\n", serde_json::to_string(&final_chunk).unwrap_or_default());
                        let _ = tx.send(Ok(sse_data)).await;
                    } else {
                        // No content found, send the choice as-is
                        let chunk = json!({
                            "choices": [first_choice],
                            "created": json_response.get("created").cloned().unwrap_or(json!(0)),
                            "id": json_response.get("id").cloned().unwrap_or(json!("unknown")),
                            "model": json_response.get("model").cloned().unwrap_or(json!("unknown")),
                            "object": "chat.completion.chunk"
                        });
                        let sse_data = format!("data: {}\n\n", serde_json::to_string(&chunk).unwrap_or_default());
                        let _ = tx.send(Ok(sse_data)).await;
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
