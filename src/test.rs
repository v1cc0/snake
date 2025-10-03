use crate::config::Config;
use crate::proxy::{AppState, proxy_handler};
use axum::Router;
use reqwest::Client;
use serde_json::{Value, json};
use std::env;
use std::net::SocketAddr;
use tracing::info;

/// Test the proxy configuration and connection
pub async fn run_test() -> Result<(), Box<dyn std::error::Error>> {
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
    println!(
        "  ├─ HOST_PORT: {}",
        env::var("HOST_PORT").unwrap_or_else(|_| "3000 (default)".to_string())
    );

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
        eprintln!(
            "Please set at least one API key (e.g., OPENAI_API_KEY, CLAUDE_API_KEY) in your .env file"
        );
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
        client,
        config: config.clone(),
    };

    let app = Router::new()
        .route("/{*path}", axum::routing::any(proxy_handler))
        .with_state(app_state);

    let addr: SocketAddr = listen_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;

    // Spawn server in background
    let server_handle = tokio::spawn(async move { axum::serve(listener, app).await });

    // Wait for server to start and verify it's listening
    for _ in 0..20 {
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        if tokio::net::TcpStream::connect(&listen_addr).await.is_ok() {
            break;
        }
    }

    println!("✓ Test server started");

    // Determine which model to use for testing
    let test_model = env::var("OPENAI_TEST_MODEL").unwrap_or_else(|_| {
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

    let mut request = test_client
        .post(&test_url)
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

    match request.send().await {
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

/// Mask a string by showing only first and last 4 characters
fn mask_string(s: &str) -> String {
    if s.len() <= 8 {
        "*".repeat(s.len())
    } else {
        format!("{}...{}", &s[..4], &s[s.len() - 4..])
    }
}

/// Mask an API key by showing only first 6 and last 4 characters
fn mask_api_key(key: &str) -> String {
    if key.len() <= 10 {
        "*".repeat(key.len())
    } else {
        format!("{}...{}", &key[..6], &key[key.len() - 4..])
    }
}
