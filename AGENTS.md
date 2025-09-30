# General Activities
- Reply in Chinese unless further notice.
- Try to precise user's requests in English, confirm with user if user's requests may lead to different technical directions which are not compatible.

# References
- Detailed progress, open tasks and upcoming actions are live in 'PROGRESS.md'.
- Before implementing requests, read 'INCIDENTS.md' to prevent incidents have happened.

# Workflow Rules
- Create a 'PROGRESS.md' file in the project root folder to record the development progress. The file including: plans, direction shifts, to-dos, completions.
- Before completing a round, update 'PROGRESS.md' with the next steps(to-dos) and achieve older notes so the file stays concise.
- Ensure 'PROGRESS.md' reflects the latest progress and is committed before handing results back.
- Commit your work at the end of each round.
- When the user says 'continue', pickup with the next planned step immediately.  
- Create 'INCIDENTS.md' file in the project root folder to record the development incidents. When user requests to record a incident report, put the record into 'INCIDENTS.md' and warm agents not to try that again. Put a very detailed incident record if the incident cause to systematic chaos or fatal failure.

# Repository Guidelines

## Project Structure & Module Organization
The proxy service lives in `src/main.rs`, combining Axum routing, Reqwest, and environment loading to forward requests to Cloudflare's AI Gateway. Dependency versions, features, and binary metadata are tracked in `Cargo.toml`; revisit it when adding crates or enabling Tokio features. Place additional modules in `src/` and expose them via an internal `mod` declaration, keeping HTTP handlers, configuration helpers, and error types in separate files for clarity. Store any future integration tests under `tests/` and name files after the behavior under inspection (for example, `tests/proxy_forwarding.rs`).

## Build, Test, and Development Commands
Run `cargo check` to validate the code quickly before committing. Use `cargo fmt` and `cargo clippy --all-targets --all-features` to enforce formatting and linting; the CI workflow will assume both pass. Start the service locally with `cargo run`; override the listening port by exporting `HOST_PORT=8080`. Produce optimized binaries with `cargo build --release` before deployment.

## Coding Style & Naming Conventions
Follow Rust's default `rustfmt` output (4-space indentation, trailing commas, and grouped imports). Favor explicit module paths over glob imports, and keep public API names descriptive (e.g., `build_client_state`). Use `snake_case` for functions and variables, `PascalCase` for types, and `CONST_CASE` for compile-time constants. Group tracing spans near the logic they explain and prefer `?` for error propagation over manual `match` chains.

## Testing Guidelines
Write unit tests beside the code with `#[cfg(test)]` modules, and use `#[tokio::test]` for async flows. When simulating proxy calls, construct requests with `axum::Router::oneshot` and assert on status codes, headers, and bodies. Target full coverage of error branches (bad configuration, network failures) and document any gaps in the pull request. Execute all suites with `cargo test` before opening a review.

## Commit & Pull Request Guidelines
Adopt Conventional Commits (`feat:`, `fix:`, `chore:`) followed by a concise summary, such as `feat: add retry logic for gateway timeouts`. Keep commits scoped to one concern and include configuration updates in the same commit as the code that depends on them. Pull requests should describe intent, highlight risky areas, list manual verification steps (`cargo run`, `cargo test`), and attach logs or screenshots when modifying observability.

## Environment & Configuration Tips
Configuration derives from environment variables; populate `.env` locally with `ACCOUNT_ID`, `GATEWAY_ID`, and optional `HOST_PORT`. Never commit real IDs or secrets—use placeholders in examples. When rotating gateway settings, update both the environment variables and any documentation that references them.
