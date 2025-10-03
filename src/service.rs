use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use tracing::info;

const SERVICE_NAME: &str = "snake.service";
const SERVICE_PATH: &str = "/etc/systemd/system/snake.service";

/// Install and start the systemd service
pub fn install_service() -> Result<(), Box<dyn std::error::Error>> {
    info!("Installing snake as systemd service...");

    // Check if running with sudo
    if env::var("USER").unwrap_or_default() != "root" && env::var("SUDO_USER").is_err() {
        eprintln!("❌ Error: This command requires sudo privileges");
        eprintln!("Please run: sudo snake service start");
        return Err("Requires sudo".into());
    }

    // Get the current binary path
    let binary_path = env::current_exe()?;
    let binary_path_str = binary_path
        .to_str()
        .ok_or("Failed to get binary path")?;

    // Get the working directory (where .env is located)
    let working_dir = env::current_dir()?;
    let working_dir_str = working_dir
        .to_str()
        .ok_or("Failed to get working directory")?;

    // Get the actual user (not root when using sudo)
    let actual_user = env::var("SUDO_USER").unwrap_or_else(|_| {
        env::var("USER").unwrap_or_else(|_| "root".to_string())
    });

    println!("📋 Service Configuration:");
    println!("  ├─ Binary: {}", binary_path_str);
    println!("  ├─ Working Directory: {}", working_dir_str);
    println!("  ├─ User: {}", actual_user);
    println!("  └─ Service File: {}", SERVICE_PATH);

    // Create systemd service file content
    let service_content = format!(
        r#"[Unit]
Description=Snake - Cloudflare AI Gateway Proxy
After=network.target

[Service]
Type=simple
User={}
WorkingDirectory={}
ExecStart={} serve
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
"#,
        actual_user, working_dir_str, binary_path_str
    );

    // Write service file
    println!("\n📝 Creating systemd service file...");
    fs::write(SERVICE_PATH, service_content)?;
    println!("✓ Service file created: {}", SERVICE_PATH);

    // Reload systemd daemon
    println!("\n🔄 Reloading systemd daemon...");
    let reload_output = Command::new("systemctl")
        .arg("daemon-reload")
        .output()?;

    if !reload_output.status.success() {
        let error = String::from_utf8_lossy(&reload_output.stderr);
        eprintln!("❌ Failed to reload systemd daemon: {}", error);
        return Err("systemctl daemon-reload failed".into());
    }
    println!("✓ Systemd daemon reloaded");

    // Enable the service
    println!("\n🔧 Enabling service (start on boot)...");
    let enable_output = Command::new("systemctl")
        .arg("enable")
        .arg(SERVICE_NAME)
        .output()?;

    if !enable_output.status.success() {
        let error = String::from_utf8_lossy(&enable_output.stderr);
        eprintln!("❌ Failed to enable service: {}", error);
        return Err("systemctl enable failed".into());
    }
    println!("✓ Service enabled");

    // Start the service
    println!("\n🚀 Starting service...");
    let start_output = Command::new("systemctl")
        .arg("start")
        .arg(SERVICE_NAME)
        .output()?;

    if !start_output.status.success() {
        let error = String::from_utf8_lossy(&start_output.stderr);
        eprintln!("❌ Failed to start service: {}", error);
        return Err("systemctl start failed".into());
    }
    println!("✓ Service started");

    // Check service status
    println!("\n📊 Service Status:");
    let status_output = Command::new("systemctl")
        .arg("status")
        .arg(SERVICE_NAME)
        .arg("--no-pager")
        .output()?;

    let status = String::from_utf8_lossy(&status_output.stdout);
    println!("{}", status);

    println!("\n✅ Snake service installed and started successfully!");
    println!("\nUseful commands:");
    println!("  sudo systemctl status snake    - Check service status");
    println!("  sudo systemctl restart snake   - Restart service");
    println!("  sudo journalctl -u snake -f    - View logs");
    println!("  sudo snake service stop        - Stop and disable service");

    Ok(())
}

/// Stop and uninstall the systemd service
pub fn uninstall_service() -> Result<(), Box<dyn std::error::Error>> {
    info!("Uninstalling snake systemd service...");

    // Check if running with sudo
    if env::var("USER").unwrap_or_default() != "root" && env::var("SUDO_USER").is_err() {
        eprintln!("❌ Error: This command requires sudo privileges");
        eprintln!("Please run: sudo snake service stop");
        return Err("Requires sudo".into());
    }

    // Check if service file exists
    if !Path::new(SERVICE_PATH).exists() {
        eprintln!("⚠️  Service file not found: {}", SERVICE_PATH);
        eprintln!("Service may not be installed or already removed.");
        return Ok(());
    }

    println!("🛑 Stopping snake service...");

    // Stop the service
    let stop_output = Command::new("systemctl")
        .arg("stop")
        .arg(SERVICE_NAME)
        .output()?;

    if !stop_output.status.success() {
        let error = String::from_utf8_lossy(&stop_output.stderr);
        // Don't fail if service is already stopped
        if !error.contains("not loaded") && !error.contains("not active") {
            eprintln!("⚠️  Warning: {}", error);
        }
    }
    println!("✓ Service stopped");

    // Disable the service
    println!("\n🔧 Disabling service...");
    let disable_output = Command::new("systemctl")
        .arg("disable")
        .arg(SERVICE_NAME)
        .output()?;

    if !disable_output.status.success() {
        let error = String::from_utf8_lossy(&disable_output.stderr);
        if !error.contains("not loaded") {
            eprintln!("⚠️  Warning: {}", error);
        }
    }
    println!("✓ Service disabled");

    // Remove service file
    println!("\n🗑️  Removing service file...");
    fs::remove_file(SERVICE_PATH)?;
    println!("✓ Service file removed: {}", SERVICE_PATH);

    // Reload systemd daemon
    println!("\n🔄 Reloading systemd daemon...");
    let reload_output = Command::new("systemctl")
        .arg("daemon-reload")
        .output()?;

    if !reload_output.status.success() {
        let error = String::from_utf8_lossy(&reload_output.stderr);
        eprintln!("⚠️  Warning: {}", error);
    }
    println!("✓ Systemd daemon reloaded");

    println!("\n✅ Snake service stopped and uninstalled successfully!");

    Ok(())
}
