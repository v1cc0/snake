use axum::{
    Router,
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use reqwest::Client;
use std::env;
use std::net::SocketAddr;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

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

    // Use the reqwest client's builder to construct and send the request
    let client_request = state.client.request(method, &target_url).headers(headers);

    let full_body = body
        .collect()
        .await
        .map_err(|e| ProxyError::BadRequest(format!("Failed to read request body: {}", e)))?;

    let response = client_request
        .body(full_body.to_bytes())
        .send()
        .await
        .map_err(|e| {
            ProxyError::BadGateway(format!("Failed to forward request to target: {}", e))
        })?;

    let mut axum_res = Response::new(Body::empty());
    *axum_res.status_mut() = response.status();
    *axum_res.headers_mut() = response.headers().clone();

    let bytes = response
        .bytes()
        .await
        .map_err(|e| ProxyError::BadGateway(format!("Failed to read response body: {}", e)))?;
    *axum_res.body_mut() = Body::from(bytes);

    Ok(axum_res)
}
