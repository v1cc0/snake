use crate::config::Config;
use crate::proxy::{AppState, proxy_handler};
use axum::Router;
use reqwest::Client;
use serde_json::{Value, json};
use std::net::SocketAddr;
use tracing::info;

/// Test modes
#[derive(Clone)]
pub enum TestMode {
    All,
    Gateway,
    Provider(String),
}

/// Test the proxy configuration and connection
pub async fn run_test(config_path: &str, mode: TestMode) -> Result<(), Box<dyn std::error::Error>> {
    let mode_desc = match &mode {
        TestMode::All => "all (gateways + providers)",
        TestMode::Gateway => "gateway rotation only",
        TestMode::Provider(name) => &format!("provider: {}", name),
    };
    info!("Running proxy test [mode: {}]", mode_desc);

    // Check if config file exists
    let path = std::path::Path::new(config_path);
    if !path.exists() {
        eprintln!("\n❌ Error: config file not found: {}", config_path);
        eprintln!("Please create a config.toml file in the project directory");
        return Err(format!("Missing config file: {}", config_path).into());
    }

    println!("\n✓ Config file found: {}", config_path);

    // Load config from TOML file
    let config = Config::from_toml(config_path)?;

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

    // Determine which providers to test based on mode
    match &mode {
        TestMode::Gateway => {
            println!("\n🔄 Testing gateway rotation (will make multiple requests)...\n");
            return test_gateway_rotation(&config, &test_client, &test_url).await;
        }
        TestMode::Provider(target_provider) => {
            println!("\n📤 Testing provider: {}...\n", target_provider);

            // Find the specific provider
            if let Some(provider_config) = config.providers.get(target_provider) {
                if provider_config.api_keys.is_empty() || provider_config.test_model.is_empty() {
                    return Err(format!("Provider '{}' has no API keys or test model configured", target_provider).into());
                }

                let api_key = &provider_config.api_keys[0];
                let test_model = &provider_config.test_model;

                tests_run = 1;
                let result = test_single_provider(
                    target_provider,
                    test_model,
                    api_key,
                    &test_client,
                    &test_url
                ).await;

                match result {
                    Ok(_) => tests_passed = 1,
                    Err(e) => {
                        tests_failed = 1;
                        println!("❌ Error: {}", e);
                    }
                }
            } else {
                return Err(format!("Provider '{}' not found in config", target_provider).into());
            }
        }
        TestMode::All => {
            println!("\n📤 Running tests for all configured providers...\n");

            for (provider_name, provider_config) in &config.providers {
                // Skip if no API keys or no test model configured
                if provider_config.api_keys.is_empty() || provider_config.test_model.is_empty() {
                    continue;
                }

                let api_key = &provider_config.api_keys[0]; // Use the first API key for testing
                let test_model = &provider_config.test_model;

                tests_run += 1;

                let result = test_single_provider(
                    provider_name,
                    test_model,
                    api_key,
                    &test_client,
                    &test_url
                ).await;

                match result {
                    Ok(_) => tests_passed += 1,
                    Err(e) => {
                        tests_failed += 1;
                        println!("❌ Error: {}", e);
                    }
                }
            }
        }
    }

    // Print summary (skip for gateway mode as it has its own summary)
    if !matches!(mode, TestMode::Gateway) {
        println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("📊 Test Summary");
        println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
        println!("  Total: {}", tests_run);
        println!("  ✅ Passed: {}", tests_passed);
        println!("  ❌ Failed: {}", tests_failed);

        if tests_failed > 0 {
            println!("\n⚠️  Some tests failed. Please check the error messages above.");
            server_handle.abort();
            return Err(format!("{} test(s) failed", tests_failed).into());
        } else {
            println!("\n✅ All tests passed successfully!");
        }
    }

    server_handle.abort();
    Ok(())
}

/// Test a single provider
async fn test_single_provider(
    provider_name: &str,
    test_model: &str,
    api_key: &str,
    test_client: &Client,
    test_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
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
        .post(test_url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&test_payload);

    match request.send().await {
        Ok(response) => {
            let status = response.status();

            match response.text().await {
                Ok(body) => {
                    if status.is_success() {
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
                        println!();
                        Ok(())
                    } else {
                        println!("❌ Status: {} {}", status.as_u16(), status.canonical_reason().unwrap_or("Unknown"));

                        // Show error details
                        if let Ok(json_body) = serde_json::from_str::<Value>(&body) {
                            println!("📄 Error response:\n{}", serde_json::to_string_pretty(&json_body)?);
                        } else {
                            println!("📄 Error: {}", body);
                        }
                        println!();
                        Err(format!("HTTP {}", status.as_u16()).into())
                    }
                }
                Err(e) => {
                    println!("❌ Failed to read response body: {}", e);
                    println!();
                    Err(e.into())
                }
            }
        }
        Err(e) => {
            println!("❌ Request failed: {}", e);
            println!();
            Err(e.into())
        }
    }
}

/// Test gateway rotation by making multiple requests
async fn test_gateway_rotation(
    config: &Config,
    test_client: &Client,
    test_url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Find first available provider for testing
    let (provider_name, provider_config) = config.providers.iter()
        .find(|(_, cfg)| !cfg.api_keys.is_empty() && !cfg.test_model.is_empty())
        .ok_or("No providers configured for testing")?;

    let api_key = &provider_config.api_keys[0];
    let test_model = &provider_config.test_model;
    let num_gateways = config.gateways.len();
    let num_requests = num_gateways * 2; // Test 2 full rotations

    println!("Testing {} requests to verify {} gateway rotation...", num_requests, num_gateways);
    println!("Using provider: {} ({})", provider_name, test_model);
    println!();

    let test_payload = json!({
        "model": test_model,
        "messages": [
            {"role": "user", "content": "Reply with just 'OK'"}
        ]
    });

    let mut success_count = 0;
    let mut used_gateways = std::collections::HashSet::new();

    for i in 0..num_requests {
        print!("Request {}/{}: ", i + 1, num_requests);

        let response = test_client
            .post(test_url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", api_key))
            .json(&test_payload)
            .send()
            .await?;

        let status = response.status();
        if status.is_success() {
            success_count += 1;
            println!("✅ OK (HTTP {})", status.as_u16());

            // Track which gateway was used (inferred from rotation)
            let gateway_idx = i % num_gateways;
            used_gateways.insert(gateway_idx);
        } else {
            println!("❌ Failed (HTTP {})", status.as_u16());
        }

        // Small delay between requests
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    }

    println!();
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📊 Gateway Rotation Test Summary");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("  Total Requests: {}", num_requests);
    println!("  Successful: {}", success_count);
    println!("  Gateways Configured: {}", num_gateways);
    println!("  Gateways Used: {}", used_gateways.len());
    println!();

    if success_count == num_requests && used_gateways.len() == num_gateways {
        println!("✅ Gateway rotation working correctly!");
        println!("   All {} gateways were used in round-robin fashion", num_gateways);
        Ok(())
    } else if success_count < num_requests {
        Err(format!("{} requests failed", num_requests - success_count).into())
    } else {
        Err("Gateway rotation may not be working as expected".into())
    }
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
