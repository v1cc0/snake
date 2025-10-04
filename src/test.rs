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

    // Check if config.toml file exists
    let config_path = std::path::Path::new("config.toml");
    if !config_path.exists() {
        eprintln!("\n❌ Error: config.toml file not found!");
        eprintln!("Please create a config.toml file in the project directory");
        return Err("Missing config.toml file".into());
    }

    println!("\n✓ config.toml file found");

    // Load config from TOML file
    let config = Config::from_toml("config.toml")?;

    // Display configuration
    println!("\n📋 Current Configuration:");
    println!("  ├─ HOST_PORT: {}", config.listen_addr.split(':').last().unwrap_or("unknown"));
    println!("  ├─ Gateways: {} configured", config.gateways.len());

    for (idx, gateway) in config.gateways.iter().enumerate() {
        println!("  │   ├─ Gateway {}: account={}, gateway_id={}, token={}",
            idx + 1,
            mask_string(&gateway.account_id),
            &gateway.gateway_id,
            mask_api_key(&gateway.token)
        );
    }

    // Check provider API keys
    println!("  └─ Provider API Keys:");
    let mut has_api_key = false;

    for (provider_name, provider_config) in &config.providers {
        if !provider_config.api_keys.is_empty() {
            println!("      ├─ {}: {} key(s) configured",
                provider_name,
                provider_config.api_keys.len()
            );
            has_api_key = true;
        } else {
            println!("      ├─ {}: ⚠️  NOT SET", provider_name);
        }
    }

    if !has_api_key {
        eprintln!("\n⚠️  Warning: No provider API key configured!");
        eprintln!("Please configure at least one provider in config.toml");
        return Err("No API key configured".into());
    }

    println!("\n✓ Configuration validated");
    // Extract port from listen_addr (format: "0.0.0.0:port")
    let port = config.listen_addr.split(':').last().unwrap_or("3000");
    let listen_addr = format!("127.0.0.1:{}", port);

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

    let test_url = format!("http://127.0.0.1:{}/v1/chat/completions", port);

    let mut tests_run = 0;
    let mut tests_passed = 0;
    let mut tests_failed = 0;

    println!("\n📤 Running tests for all configured providers...\n");

    for (provider_name, provider_config) in &config.providers {
        // Skip if no API keys or no test model configured
        if provider_config.api_keys.is_empty() || provider_config.test_model.is_empty() {
            continue;
        }

        let api_key = &provider_config.api_keys[0]; // Use the first API key for testing
        let test_model = &provider_config.test_model;

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

        let request = test_client
            .post(&test_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&test_payload);

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
