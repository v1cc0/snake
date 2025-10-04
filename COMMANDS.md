# Snake CLI Command Reference

Complete list of all available commands and their parameters.

## Global Options

```bash
-c, --config <CONFIG>  # Path to config file (default: config.toml)
-h, --help             # Print help
-V, --version          # Print version
```

## Commands Overview

```bash
snake [OPTIONS] [COMMAND]

Commands:
  update    Check for updates and upgrade to the latest version
  serve     Start the proxy server (default)
  test      Test the proxy configuration and connection
  config    Configuration management
  service   Manage systemd service
  help      Print help message
```

---

## 1. update - Update to latest version

```bash
snake update [OPTIONS]

Options:
  -y, --yes              Skip confirmation prompt
  -t, --token <TOKEN>    GitHub personal access token
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
snake update                    # Interactive update
snake update -y                 # Auto-confirm update
snake update --token ghp_xxx    # Use GitHub token
```

---

## 2. serve - Start proxy server

```bash
snake serve [OPTIONS]
# Or simply:
snake [OPTIONS]  # serve is default command

Options:
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
snake                              # Start with config.toml
snake serve                        # Same as above
snake --config /etc/snake/prod.toml  # Custom config
```

---

## 3. test - Test proxy functionality

### 3.1 Test all (default)

```bash
snake test [OPTIONS]
snake test all [OPTIONS]

Options:
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
snake test              # Test all providers
snake test all          # Same as above
```

### 3.2 Test gateway rotation

```bash
snake test gateway [OPTIONS]

Options:
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
snake test gateway      # Test gateway round-robin
```

### 3.3 Test specific provider

```bash
snake test provider [OPTIONS] <NAME>

Arguments:
  <NAME>  Provider name (e.g., openai, google-ai-studio, groq)

Options:
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
snake test provider openai              # Test OpenAI with all configured keys
snake test provider google-ai-studio    # Test Google AI Studio with all keys
snake test provider groq                # Test Groq with all keys
```

---

## 4. config - Configuration management

### 4.1 Check configuration

```bash
snake config check [OPTIONS] [PATH]

Arguments:
  [PATH]  Path to config file to check (overrides --config)

Options:
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
snake config check                      # Check config.toml
snake config check /etc/snake/prod.toml # Check specific file
snake --config custom.toml config check # Use global --config
```

---

## 5. service - Systemd service management

### 5.1 Install and start service

```bash
snake service start [OPTIONS]

Options:
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
sudo snake service start  # Install and start systemd service
```

### 5.2 Stop and uninstall service

```bash
snake service stop [OPTIONS]

Options:
  -c, --config <CONFIG>  Config file path
  -h, --help             Print help
```

**Examples:**
```bash
sudo snake service stop   # Stop and remove systemd service
```

---

## Complete Usage Examples

### Testing workflow
```bash
# 1. Check configuration is valid
snake config check

# 2. Test gateway rotation
snake test gateway

# 3. Test specific provider
snake test provider openai

# 4. Test all providers
snake test all
```

### Production deployment
```bash
# 1. Validate production config
snake --config /etc/snake/prod.toml config check

# 2. Install as systemd service
sudo snake --config /etc/snake/prod.toml service start

# 3. Check service status
systemctl status snake
```

### Development workflow
```bash
# 1. Start with custom config
snake --config dev.toml serve

# 2. Test in another terminal
snake --config dev.toml test all
```

---

## Quick Reference

| Task | Command |
|------|---------|
| Start proxy | `snake` or `snake serve` |
| Custom config | `snake --config path/to/config.toml` |
| Test all | `snake test` |
| Test gateway | `snake test gateway` |
| Test provider | `snake test provider <name>` |
| Check config | `snake config check` |
| Update | `snake update` |
| Install service | `sudo snake service start` |
| Stop service | `sudo snake service stop` |
| Show help | `snake --help` |
| Show version | `snake --version` |

---

## Provider Names

Available providers (as configured in config.toml):

- `openai`
- `google-ai-studio`
- `groq`
- `mistral`
- `cohere`
- `anthropic`
- `xai`

**Note:** Provider name must match the key in `[providers.<name>]` section of config.toml.
