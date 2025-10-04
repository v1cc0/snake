# Snake - the API proxy

A lightweight, high-performance HTTP proxy that forwards OpenAI-compatible requests to Cloudflare's AI Gateway. Built with [Axum](https://github.com/tokio-rs/axum), [Tokio](https://tokio.rs/), and [Reqwest](https://github.com/seanmonstar/reqwest).

## Features
- **OpenAI-Compatible API**: Drop-in replacement for OpenAI endpoints
- **Native HTTPS/TLS Support**: Built-in HTTPS server with rustls (no reverse proxy needed)
- **Multi-Gateway Load Balancing**: Round-robin rotation across multiple Cloudflare AI Gateways
- **Multi-Key Rotation**: Automatic API key rotation per provider for rate limit handling
- **SSE Streaming**: Simulates word-by-word streaming from non-stream Cloudflare responses
- **Advanced Testing**: Test all providers, gateways, or specific provider keys
- **Auto-Update**: Built-in self-update from GitHub releases
- **Configuration Management**: TOML-based config with validation
- **Systemd Integration**: Service management with auto-restart
- **Structured Logging**: Comprehensive tracing via `tracing` crate

## Prerequisites
- **For running the binary**: Linux x86_64 with GLIBC 2.35+ (Ubuntu 22.04+, Debian 12+, etc.)
- **For building from source**: Rust toolchain (1.77 or later is recommended; the crate targets the Rust 2024 edition).
- A Cloudflare account with an AI Gateway configured and the corresponding `ACCOUNT_ID` and `GATEWAY_ID` values.

### GLIBC Compatibility Note
Pre-built binaries require GLIBC 2.35 or later (Ubuntu 22.04+, Debian 12+). If you encounter `GLIBC_X.XX not found` errors:
1. **Build from source** on your system: `make build`
2. Or use a newer Linux distribution

The GitHub releases are built on Ubuntu 22.04 (GLIBC 2.35) for broad compatibility.

## Installation

### Option 1: Download Binary (Recommended)
```bash
# Download latest release from GitHub
# https://github.com/v1cc0/snake/releases

# Make executable and move to PATH
chmod +x snake
sudo mv snake /usr/local/bin/
```

### Option 2: Build from Source
```bash
git clone https://github.com/v1cc0/snake.git
cd snake
make build  # Binary will be copied to ./snake
```

## Configuration

Create a `config.toml` file by copying the template:

```bash
cp config.toml.template config.toml
# Edit config.toml with your credentials
```

**Configuration Structure:**

```toml
# Server settings
host_port = 3000

# HTTPS Configuration (optional)
# Set https_server = true to enable native HTTPS/TLS support
https_server = false
tls_cert_path = "cert.pem"
tls_key_path = "key.pem"

# Cloudflare AI Gateway configurations (rotated in round-robin)
[[gateways]]
account_id = "your-cloudflare-account-id-1"
gateway_id = "your-gateway-id"
token = "your-gateway-token-1"

[[gateways]]
account_id = "your-cloudflare-account-id-2"
gateway_id = "your-gateway-id"
token = "your-gateway-token-2"

# Provider API Keys (rotated per provider in round-robin)
[providers.openai]
api_keys = ["sk-proj-your-openai-api-key"]
test_model = "openai/gpt-4o-mini"

[providers.google-ai-studio]
api_keys = [
  "AIzaSy-your-google-api-key-1",
  "AIzaSy-your-google-api-key-2"  # Multiple keys for rotation
]
test_model = "google-ai-studio/gemini-2.0-flash-exp"

[providers.anthropic]
api_keys = ["sk-ant-your-anthropic-api-key"]
test_model = "anthropic/claude-3-5-sonnet-20241022"

# Other providers: groq, mistral, cohere, xai
```

**Required Configuration:**
- At least one gateway in `[[gateways]]` array
- At least one provider with `api_keys` array

**HTTPS Configuration (Optional):**
- Set `https_server = true` to enable native HTTPS/TLS
- Provide paths to TLS certificate and private key files
- Supports both PKCS8 and PKCS1 private key formats
- HTTP/1.1 and HTTP/2 are enabled via ALPN
- Default is HTTP mode (`https_server = false`)

**Multi-Gateway Load Balancing:**
- Add multiple `[[gateways]]` entries to distribute requests across different Cloudflare accounts/gateways
- Requests are automatically rotated in round-robin fashion

**Multi-Key Rotation:**
- Configure multiple keys per provider in the `api_keys` array
- Keys are automatically rotated per provider to handle rate limits

## Usage

### CLI Commands

**Test Configuration**
```bash
# Test all configured providers (default)
snake test
snake test all

# Test gateway rotation (2x full rotations)
snake test gateway

# Test specific provider with ALL configured API keys
snake test provider openai
snake test provider google-ai-studio
```

The test command supports multiple modes:
- **all** (default): Tests all providers with configured API keys and test models
- **gateway**: Tests gateway round-robin rotation (makes 2x full rotations)
- **provider <name>**: Tests ALL API keys for a specific provider (openai, google-ai-studio, anthropic, groq, mistral, cohere, xai)

Each test validates:
- Configuration file syntax and requirements
- Network connectivity to Cloudflare AI Gateway
- API key validity and provider response
- Individual key masking for security

**Validate Configuration**
```bash
# Check default config.toml
snake config check

# Check custom config file
snake config check /path/to/config.toml
```

**Use Custom Config File**
```bash
# Global --config flag works with all commands
snake --config /etc/snake/prod.toml serve
snake --config /etc/snake/prod.toml test all
```

**Update to Latest Version**
```bash
snake update                          # Interactive prompt
snake update -y                       # Auto-confirm
snake update --token "ghp_xxxxx"      # Use GitHub token (for rate limit)
snake update -y --token "ghp_xxxxx"   # Combine options
```

The update command will:
- Download the latest binary from GitHub releases
- Automatically detect if `snake.service` is running
- Restart the service with the new version (if running)
- If service exists but not running, prompt you to start it manually

Note: Store GitHub token in `config.toml` or set `GITHUB_TOKEN` environment variable to avoid rate limiting.

**Start Proxy Server**
```bash
snake serve         # Or just: snake
```

**Manage Systemd Service**
```bash
# Install and start as systemd service (requires sudo)
sudo snake service start

# Stop and remove systemd service
sudo snake service stop

# Check service status
sudo systemctl status snake

# View service logs
sudo journalctl -u snake -f
```

The service will:
- Start automatically on system boot
- Restart automatically if it crashes (Restart=always)
- Run as the current user (preserves .env access)
- Use the current working directory (where .env is located)

### Making Requests

The proxy exposes an OpenAI-compatible endpoint at `http://localhost:{HOST_PORT}/v1/chat/completions`.

**Non-streaming request:**
```bash
curl http://localhost:38388/v1/chat/completions \
  -H "cf-aig-authorization: Bearer $CF_AIG_TOKEN" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-5-2025-08-07",
    "messages": [{"role": "user", "content": "Hello!"}]
  }'
```

**Note**: Model must be in `provider/model` format. Examples:
- OpenAI: `openai/gpt-5-2025-08-07`, `openai/gpt-4o-mini`
- Anthropic: `anthropic/claude-3-5-sonnet-20241022`
- Google: `google/gemini-2.5-flash`
- Groq: `groq/openai/gpt-oss-120b`
- Mistral: `mistral/mistral-large-latest`
- Cohere: `cohere/command-a-reasoning-08-2025`

**Streaming request (SSE):**
```bash
curl http://localhost:38388/v1/chat/completions \
  -H "cf-aig-authorization: Bearer $CF_AIG_TOKEN" \
  -H "Authorization: Bearer $OPENAI_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "model": "openai/gpt-5-2025-08-07",
    "messages": [{"role": "user", "content": "What is AI?"}],
    "stream": true
  }'
```

**How streaming works:**
1. Client sends request with `"stream": true`
2. Proxy modifies to `"stream": false` for Cloudflare (CF doesn't support SSE)
3. Proxy receives complete response from Cloudflare
4. Proxy converts to SSE format with word-by-word streaming
5. Client sees progressive text output with proper OpenAI SSE format

## Development

### Build Commands
```bash
make build    # Build release binary to ./snake
make clean    # Clean build artifacts
make test     # Run tests
make check    # Run cargo check
make fmt      # Format code
```

### Development Workflow
```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo check
cargo build --release
```

## Architecture

```
Client Request (stream: true, model: "openai/gpt-5-2025-08-07")
    ↓
Snake Proxy (modify stream: false)
    ↓
Cloudflare AI Gateway (/compat/chat/completions)
    ↓
Provider API (OpenAI/Claude/Gemini/Groq/Mistral/Cohere/etc., complete response)
    ↓
Cloudflare AI Gateway
    ↓
Snake Proxy (convert to SSE, word-by-word streaming)
    ↓
Client (receives SSE stream)
```

**Module Structure (v0.1.0+):**
```
src/
├── main.rs       312 lines - CLI entry, routing, config check
├── config.rs     136 lines - TOML config + round-robin rotation
├── update.rs     198 lines - Self-update + service restart
├── test.rs       474 lines - Advanced multi-mode testing
├── proxy.rs      205 lines - Request forwarding + key rotation
├── stream.rs     145 lines - SSE conversion
└── service.rs    203 lines - Systemd integration
```

**Key Components:**
- **Config Manager** (config.rs): TOML parsing, multi-gateway and multi-key round-robin rotation
- **Request Handler** (proxy.rs): Filters headers, provider detection, automatic API key rotation
- **SSE Converter** (stream.rs): Splits complete response into word-by-word chunks
- **Update Manager** (update.rs): GitHub release integration with automatic service restart
- **Multi-Mode Tester** (test.rs): Tests all providers, gateways, or specific provider keys
- **Service Manager** (service.rs): Systemd service installation and management

## Logging

Configured at `INFO` level by default. Set `RUST_LOG` environment variable for custom levels:

```bash
RUST_LOG=debug snake serve
RUST_LOG=snake=trace,axum=debug snake serve
```

## Error Handling

- **400 Bad Request**: Missing/invalid configuration
- **502 Bad Gateway**: Cloudflare forwarding failures
- All errors logged with full context for troubleshooting

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make changes with tests
4. Run `make fmt` and `make check`
5. Submit pull request

## License

MIT License - See LICENSE file for details
