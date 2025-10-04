use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::info;

/// Single gateway configuration
#[derive(Debug, Clone, Deserialize)]
pub struct GatewayConfig {
    pub account_id: String,
    pub gateway_id: String,
    pub token: String,
}

impl GatewayConfig {
    /// Construct the full Cloudflare AI Gateway URL for this gateway
    pub fn base_url(&self) -> String {
        format!(
            "https://gateway.ai.cloudflare.com/v1/{}/{}",
            self.account_id, self.gateway_id
        )
    }
}

/// Provider-specific configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ProviderConfig {
    #[serde(default)]
    pub api_keys: Vec<String>,
    #[serde(default)]
    pub test_model: String,
}

/// Complete configuration loaded from config.toml
#[derive(Debug, Deserialize)]
pub struct TomlConfig {
    #[serde(default = "default_port")]
    pub host_port: u16,
    #[serde(default = "default_https_port")]
    pub https_port: u16,
    #[serde(default)]
    pub https_server: bool,
    #[serde(default = "default_cert_path")]
    pub tls_cert_path: String,
    #[serde(default = "default_key_path")]
    pub tls_key_path: String,
    pub gateways: Vec<GatewayConfig>,
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

fn default_port() -> u16 {
    3000
}

fn default_https_port() -> u16 {
    443
}

fn default_cert_path() -> String {
    "cert.pem".to_string()
}

fn default_key_path() -> String {
    "key.pem".to_string()
}

/// Runtime configuration with round-robin state
#[derive(Clone)]
pub struct Config {
    pub listen_addr: String,
    pub http_port: u16,
    pub https_port: u16,
    pub https_server: bool,
    pub tls_cert_path: String,
    pub tls_key_path: String,
    pub gateways: Vec<GatewayConfig>,
    pub providers: HashMap<String, ProviderConfig>,
    pub openai_compat_path: String,
    gateway_counter: Arc<AtomicUsize>,
    provider_counters: HashMap<String, Arc<AtomicUsize>>,
}

impl Config {
    /// Load configuration from config.toml file
    pub fn from_toml(path: &str) -> Result<Self, String> {
        info!("Loading configuration from: {}", path);

        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read config file {}: {}", path, e))?;

        let toml_config: TomlConfig = toml::from_str(&content)
            .map_err(|e| format!("Failed to parse TOML config: {}", e))?;

        if toml_config.gateways.is_empty() {
            return Err("At least one gateway configuration is required".to_string());
        }

        info!("Loaded {} gateway(s) from config", toml_config.gateways.len());
        for (idx, gateway) in toml_config.gateways.iter().enumerate() {
            info!(
                "  Gateway {}: account_id={}, gateway_id={}",
                idx + 1,
                gateway.account_id,
                gateway.gateway_id
            );
        }

        // Initialize provider counters
        let mut provider_counters = HashMap::new();
        for (name, provider) in &toml_config.providers {
            if !provider.api_keys.is_empty() {
                info!("Provider '{}': {} API key(s)", name, provider.api_keys.len());
                provider_counters.insert(name.clone(), Arc::new(AtomicUsize::new(0)));
            }
        }

        // Use https_port when HTTPS is enabled, otherwise use host_port
        let port = if toml_config.https_server {
            toml_config.https_port
        } else {
            toml_config.host_port
        };
        let listen_addr = format!("0.0.0.0:{}", port);

        Ok(Self {
            listen_addr,
            http_port: toml_config.host_port,
            https_port: toml_config.https_port,
            https_server: toml_config.https_server,
            tls_cert_path: toml_config.tls_cert_path,
            tls_key_path: toml_config.tls_key_path,
            gateways: toml_config.gateways,
            providers: toml_config.providers,
            openai_compat_path: "/compat/chat/completions".to_string(),
            gateway_counter: Arc::new(AtomicUsize::new(0)),
            provider_counters,
        })
    }

    /// Get the next gateway using round-robin rotation
    pub fn next_gateway(&self) -> &GatewayConfig {
        let index = self.gateway_counter.fetch_add(1, Ordering::Relaxed) % self.gateways.len();
        &self.gateways[index]
    }

    /// Get the next API key for a specific provider using round-robin rotation
    pub fn next_api_key(&self, provider: &str) -> Option<String> {
        let provider_config = self.providers.get(provider)?;
        if provider_config.api_keys.is_empty() {
            return None;
        }

        let counter = self.provider_counters.get(provider)?;
        let index = counter.fetch_add(1, Ordering::Relaxed) % provider_config.api_keys.len();
        Some(provider_config.api_keys[index].clone())
    }

    /// Get the full target URL for the next gateway
    pub fn next_target_url(&self) -> String {
        let gateway = self.next_gateway();
        format!("{}{}", gateway.base_url(), self.openai_compat_path)
    }

    /// Get the cf-aig-authorization token for the current gateway
    pub fn current_gateway_token(&self) -> &str {
        // Get the same gateway that was just selected
        let index = (self.gateway_counter.load(Ordering::Relaxed).wrapping_sub(1)) % self.gateways.len();
        &self.gateways[index].token
    }
}
