use std::env;
use tracing::info;

/// Configuration structure loaded from environment variables
#[derive(Clone)]
pub struct Config {
    /// Cloudflare AI Gateway base URL (e.g., https://gateway.ai.cloudflare.com/v1/{account}/{gateway})
    pub cf_base_gateway_url: String,
    /// The path segment for the provider (e.g., /openai)
    pub openai_compat_path: String,
    /// The address to listen on (e.g., 0.0.0.0:3000)
    pub listen_addr: String,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self, String> {
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
