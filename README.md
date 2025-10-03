# Snake - Cloudflare AI Gateway Proxy

A lightweight, high-performance HTTP proxy that forwards OpenAI-compatible requests to Cloudflare's AI Gateway. Built with [Axum](https://github.com/tokio-rs/axum), [Tokio](https://tokio.rs/), and [Reqwest](https://github.com/seanmonstar/reqwest).

## Features
- **OpenAI-Compatible API**: Drop-in replacement for OpenAI endpoints
- **SSE Streaming**: Simulates word-by-word streaming from non-stream Cloudflare responses
- **Auto-Update**: Built-in self-update from GitHub releases
- **Configuration Test**: Validate setup before deployment
- **Header Management**: Proper HTTP header filtering and forwarding
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

Create a `.env` file in your working directory:

```env
HOST_PORT=38388
ACCOUNT_ID=your_cloudflare_account_id
GATEWAY_ID=your_gateway_id
CF_AIG_TOKEN=your_cloudflare_ai_gateway_token

# OpenAI Configuration
OPENAI_API_KEY=your_openai_api_key
OPENAI_TEST_MODEL=openai/gpt-5-2025-08-07

# Anthropic Configuration (optional)
ANTHROPIC_API_KEY=your_anthropic_api_key
ANTHROPIC_TEST_MODEL=anthropic/claude-3-5-sonnet-20241022

# Google AI Studio Configuration (optional)
GOOGLEAISTUDIO_API_KEY=your_google_api_key
GOOGLEAISTUDIO_TEST_MODEL=google-ai-studio/gemini-2.5-flash

# Groq Configuration (optional)
GROQ_API_KEY=your_groq_api_key
GROQ_TEST_MODEL=groq/openai/gpt-oss-120b

# Mistral Configuration (optional)
MISTRAL_API_KEY=your_mistral_api_key
MISTRAL_TEST_MODEL=mistral/mistral-large-latest

# Cohere Configuration (optional)
COHERE_API_KEY=your_cohere_api_key
COHERE_TEST_MODEL=cohere/command-a-reasoning-08-2025

# XAI Configuration (optional)
XAI_API_KEY=your_xai_api_key
XAI_TEST_MODEL=xai/grok-beta
```

**Required Variables:**
- `ACCOUNT_ID`: Your Cloudflare account ID
- `GATEWAY_ID`: Your AI Gateway ID
- `CF_AIG_TOKEN`: Cloudflare AI Gateway authentication token
- At least one provider API key (e.g., `OPENAI_API_KEY`)

**Optional Variables:**
- `HOST_PORT`: Port to listen on (default: 3000)
- `GITHUB_TOKEN`: GitHub personal access token for updates (avoids rate limiting)
- Test model variables (for multi-provider testing):
  - `OPENAI_TEST_MODEL`: OpenAI test model (default: openai/gpt-5-2025-08-07)
  - `ANTHROPIC_TEST_MODEL`: Anthropic test model
  - `GOOGLEAISTUDIO_TEST_MODEL`: Google AI Studio test model
  - `GROQ_TEST_MODEL`: Groq test model
  - `MISTRAL_TEST_MODEL`: Mistral test model
  - `COHERE_TEST_MODEL`: Cohere test model
  - `XAI_TEST_MODEL`: XAI test model

## Usage

### CLI Commands

**Test Configuration**
```bash
snake test
```
Validates your `.env` configuration and tests all configured providers.

The test command will:
- Validate `.env` file existence and required configuration
- Test all providers that have both API key and test model configured
- Display individual test results for each provider
- Show a summary of passed/failed tests

Supported test model environment variables:
- `OPENAI_TEST_MODEL` with `OPENAI_API_KEY`
- `ANTHROPIC_TEST_MODEL` with `ANTHROPIC_API_KEY`
- `GOOGLEAISTUDIO_TEST_MODEL` with `GOOGLEAISTUDIO_API_KEY`
- `GROQ_TEST_MODEL` with `GROQ_API_KEY`
- `MISTRAL_TEST_MODEL` with `MISTRAL_API_KEY`
- `COHERE_TEST_MODEL` with `COHERE_API_KEY`
- `XAI_TEST_MODEL` with `XAI_API_KEY`

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

Note: GitHub token can also be set via `GITHUB_TOKEN` in `.env` file to avoid rate limiting.

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
├── main.rs       ~160 lines - CLI entry point and routing
├── config.rs      44 lines  - Configuration management
├── update.rs     140 lines  - Self-update functionality
├── test.rs       265 lines  - Multi-provider testing
├── proxy.rs      180 lines  - Request proxy handler
├── stream.rs     145 lines  - SSE streaming conversion
└── service.rs    180 lines  - Systemd service management
```

**Key Components:**
- **Request Handler** (proxy.rs): Filters hop-by-hop headers, modifies body
- **SSE Converter** (stream.rs): Splits complete response into word-by-word chunks
- **Update Manager** (update.rs): GitHub release integration with semver comparison
- **Multi-Provider Tester** (test.rs): Validates configuration and tests all providers
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
