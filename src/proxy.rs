use crate::config::Config;
use crate::stream::convert_to_sse_stream;
use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use http_body_util::BodyExt;
use reqwest::Client;
use serde_json::{Value, json};
use tracing::{error, info};

/// Custom error type for proxy operations
pub enum ProxyError {
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

/// Application state holding the HTTP client and configuration
#[derive(Clone)]
pub struct AppState {
    pub client: Client,
    pub config: Config,
}

/// Main proxy handler that forwards requests to Cloudflare AI Gateway
pub async fn proxy_handler(
    State(state): State<AppState>,
    req: Request<Body>,
) -> Result<Response, ProxyError> {
    let (parts, body) = req.into_parts();
    let method = parts.method;
    let headers = parts.headers;

    // Get the next gateway in round-robin fashion
    let target_url = state.config.next_target_url();
    let gateway_token = state.config.current_gateway_token();

    info!("Forwarding request to: {} {} (round-robin)", method, target_url);

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
    let (modified_body, was_stream_request) =
        if let Ok(mut json_body) = serde_json::from_slice::<Value>(&body_bytes) {
            let was_stream = json_body
                .get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if was_stream {
                info!("Detected stream request, converting to non-stream for Cloudflare");
                json_body["stream"] = json!(false);
                let modified = serde_json::to_vec(&json_body).map_err(|e| {
                    ProxyError::BadRequest(format!("Failed to serialize modified body: {}", e))
                })?;
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
    filtered_headers.remove("host"); // reqwest will set this based on target URL
    filtered_headers.remove("content-length"); // reqwest will set this based on body size
    filtered_headers.remove("connection");
    filtered_headers.remove("keep-alive");
    filtered_headers.remove("proxy-authenticate");
    filtered_headers.remove("proxy-authorization");
    filtered_headers.remove("te");
    filtered_headers.remove("trailers");
    filtered_headers.remove("transfer-encoding");
    filtered_headers.remove("upgrade");

    // Set the gateway token for authentication
    let token_value = format!("Bearer {}", gateway_token);
    filtered_headers.insert(
        "cf-aig-authorization",
        token_value.parse().map_err(|e| {
            ProxyError::BadRequest(format!("Invalid gateway token format: {}", e))
        })?,
    );

    info!("Sending request to Cloudflare...");
    if was_stream_request {
        info!(
            "Modified body for non-streaming request, new size: {} bytes",
            modified_body.len()
        );
    }
    let client_request = state
        .client
        .request(method, &target_url)
        .headers(filtered_headers)
        .body(modified_body);
    let response = client_request.send().await.map_err(|e| {
        error!("Failed to forward request to Cloudflare: {}", e);
        ProxyError::BadGateway(format!("Failed to forward request to target: {}", e))
    })?;

    info!(
        "Received response from Cloudflare, status: {}",
        response.status()
    );

    let status = response.status();
    let response_headers = response.headers().clone();

    let bytes = response.bytes().await.map_err(|e| {
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

    Ok(axum_res)
}
