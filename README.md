# Cloudflare AI Gateway Proxy

This project provides a lightweight HTTP proxy that forwards traffic to Cloudflare's AI Gateway using the OpenAI compatibility path. It is built with [Axum](https://github.com/tokio-rs/axum), [Tokio](https://tokio.rs/), and [Reqwest](https://github.com/seanmonstar/reqwest), making it easy to deploy a Cloudflare-compatible proxy behind your own network boundary.

## Overview
- Accepts any HTTP method on any path and forwards it to Cloudflare AI Gateway.
- Rebuilds the outgoing URL as `https://gateway.ai.cloudflare.com/v1/{ACCOUNT_ID}/{GATEWAY_ID}/compat/*`.
- Preserves headers and status codes from Cloudflare responses for full API compatibility.
- Emits structured logs via `tracing` to simplify debugging and monitoring.

## Prerequisites
- Rust toolchain (1.77 or later is recommended; the crate targets the Rust 2024 edition).
- A Cloudflare account with an AI Gateway configured and the corresponding `ACCOUNT_ID` and `GATEWAY_ID` values.

## Local Setup
1. Clone the repository and change into the project directory.
2. Create a `.env` file (optional but recommended) with the required environment variables:

   ```env
   ACCOUNT_ID=your_cloudflare_account_id
   GATEWAY_ID=your_ai_gateway_id
   # Optional: override the port exposed by the proxy
   HOST_PORT=3000
   ```

   `HOST_PORT` defaults to `3000` when not provided. The proxy listens on `0.0.0.0:{HOST_PORT}`.

3. Install dependencies using `cargo` (the Rust package manager). All dependencies are defined in `Cargo.toml` and will be fetched automatically during the first build.

## Running the Proxy
Start the service locally:

```bash
cargo run
```

Set a custom port by exporting `HOST_PORT` before running:

```bash
HOST_PORT=8080 cargo run
```

Once running, send requests to the proxy just as you would to the OpenAI-compatible endpoint. Example using `curl`:

```bash
curl \
  -H "Authorization: Bearer $YOUR_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"model": "@cf/meta/llama-3-8b-instruct", "messages": [{"role": "user", "content": "Hello"}]}' \
  http://localhost:3000/v1/chat/completions
```

The proxy forwards this request to `https://gateway.ai.cloudflare.com/v1/{ACCOUNT_ID}/{GATEWAY_ID}/compat/v1/chat/completions` using a pooled `reqwest::Client`.

## Development Workflow
Run the standard Rust tooling before committing changes:

```bash
cargo fmt
cargo clippy --all-targets --all-features
cargo check
```

Execute tests (add your own in `tests/` or inline `#[cfg(test)]` modules):

```bash
cargo test
```

Build an optimized binary for deployment:

```bash
cargo build --release
```

## Project Structure
- `src/main.rs`: Application entry point, router definition, configuration loader, and proxy handler.
- `Cargo.toml`: Crate metadata as well as dependency versions and features.

## Logging & Observability
`tracing` is configured at `INFO` level by default. Logs highlight environment loading, the gateway target URL, and proxy forwarding decisions. Integrate with your preferred log collector by configuring `RUST_LOG` and extending the `FmtSubscriber` if needed.

## Error Handling
The proxy surfaces configuration and forwarding issues as HTTP error responses:
- Missing environment variables return `400 Bad Request` before the server starts.
- Network failures or unexpected response issues return `502 Bad Gateway` to the client in the handler.

These errors are logged with context to aid troubleshooting.

## Limitations & Future Enhancements
- Requests and responses are fully buffered in memory; streaming is not yet supported.
- Authentication, rate limiting, and additional observability hooks can be layered on top of this proxy depending on deployment requirements.

Contributions are welcome—feel free to open issues or submit pull requests to extend the proxy.
