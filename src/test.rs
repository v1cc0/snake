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
        ("Anthropic", "ANTHROPIC_API_KEY"),
        ("Google AI Studio", "GOOGLEAISTUDIO_API_KEY"),
        ("Groq", "GROQ_API_KEY"),
        ("Mistral", "MISTRAL_API_KEY"),
        ("XAI", "XAI_API_KEY"),
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

    // Collect all providers with both API key and test model configured
    let providers = vec![
        ("OpenAI", "OPENAI_API_KEY", "OPENAI_TEST_MODEL"),
        ("Anthropic", "ANTHROPIC_API_KEY", "ANTHROPIC_TEST_MODEL"),
        ("Google AI Studio", "GOOGLEAISTUDIO_API_KEY", "GOOGLEAISTUDIO_TEST_MODEL"),
        ("Groq", "GROQ_API_KEY", "GROQ_TEST_MODEL"),
        ("Mistral", "MISTRAL_API_KEY", "MISTRAL_TEST_MODEL"),
        ("XAI", "XAI_API_KEY", "XAI_TEST_MODEL"),
    ];

    let cf_aig_token = env::var("CF_AIG_TOKEN").unwrap_or_default();
    let test_url = format!("http://127.0.0.1:{}/v1/chat/completions", host_port);

    let mut tests_run = 0;
    let mut tests_passed = 0;
    let mut tests_failed = 0;

    println!("\n📤 Running tests for all configured providers...\n");

    for (provider_name, api_key_env, test_model_env) in providers {
        let api_key = env::var(api_key_env).unwrap_or_default();
        let test_model = env::var(test_model_env).unwrap_or_default();

        // Skip if either API key or test model is not configured
        if api_key.is_empty() || test_model.is_empty() {
            continue;
        }

        tests_run += 1;
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("🧪 Testing {} ({})", provider_name, test_model);
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

        let test_payload = json!({
            "model": test_model,
            "messages": [
                {"role": "user", "content": "Say 'Hello from provider!' in one short sentence."}
            ]
        });

        let mut request = test_client
            .post(&test_url)
            .header("Content-Type", "application/json")
            .json(&test_payload);

        // Add cf-aig-authorization header if CF_AIG_TOKEN is set
        if !cf_aig_token.is_empty() {
            request = request.header("cf-aig-authorization", format!("Bearer {}", cf_aig_token));
        }

        // Add Authorization header with provider API key
        request = request.header("Authorization", format!("Bearer {}", api_key));

        match request.send().await {
            Ok(response) => {
                let status = response.status();

                match response.text().await {
                    Ok(body) => {
                        if status.is_success() {
                            tests_passed += 1;
                            println!("✅ Status: {} OK", status.as_u16());

                            // Try to extract and show the message content
                            if let Ok(json_body) = serde_json::from_str::<Value>(&body) {
                                if let Some(content) = json_body["choices"][0]["message"]["content"].as_str() {
                                    println!("📝 Response: {}", content);
                                } else {
                                    println!("📄 Full response:\n{}", serde_json::to_string_pretty(&json_body)?);
                                }
                            } else {
                                println!("📄 Response: {}", body);
                            }
                        } else {
                            tests_failed += 1;
                            println!("❌ Status: {} {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown"));

                            // Show error details
                            if let Ok(json_body) = serde_json::from_str::<Value>(&body) {
                                println!("📄 Error response:\n{}", serde_json::to_string_pretty(&json_body)?);
                            } else {
                                println!("📄 Error: {}", body);
                            }
                        }
                    }
                    Err(e) => {
                        tests_failed += 1;
                        println!("❌ Failed to read response body: {}", e);
                    }
                }
            }
            Err(e) => {
                tests_failed += 1;
                println!("❌ Request failed: {}", e);
            }
        }
        println!();
    }

    if tests_run == 0 {
        eprintln!("⚠️  No providers configured for testing!");
        eprintln!("Please set at least one pair of API_KEY and TEST_MODEL environment variables.");
        return Err("No providers configured".into());
    }

    // Print summary
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 Test Summary");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Total: {}", tests_run);
    println!("  ✅ Passed: {}", tests_passed);
    println!("  ❌ Failed: {}", tests_failed);
    println!();

    if tests_failed > 0 {
        println!("⚠️  Some tests failed. Please check the error messages above.");
        return Err(format!("{} test(s) failed", tests_failed).into());
    }

    println!("✅ All tests passed successfully!");


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
