use std::env;
use tracing::info;

/// Check for updates and install if available
pub async fn check_and_update(
    version: &str,
    repo_owner: &str,
    repo_name: &str,
    skip_confirm: bool,
    token: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("Current version: {}", version);
    info!(
        "Checking for updates from GitHub repository: {}/{}",
        repo_owner, repo_name
    );

    // Use token from CLI argument, or fall back to GITHUB_TOKEN env var
    dotenvy::dotenv().ok();
    let github_token = token.or_else(|| env::var("GITHUB_TOKEN").ok());

    if github_token.is_some() {
        info!("Using GitHub token for API requests");
    }

    let status = if let Some(ref token) = github_token {
        self_update::backends::github::Update::configure()
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(version)
            .auth_token(token)
            .build()?
    } else {
        self_update::backends::github::Update::configure()
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(version)
            .build()?
    };

    let latest_release = status.get_latest_release()?;
    let latest_version = latest_release.version.trim_start_matches('v');

    info!("Latest version available: {}", latest_version);

    // Check if versions are exactly the same
    if version == latest_version {
        info!("You are already running the latest version!");
        return Ok(());
    }

    // Try to parse and compare versions using semver
    let needs_update = match (
        semver::Version::parse(version),
        semver::Version::parse(latest_version),
    ) {
        (Ok(current), Ok(latest)) => {
            // Compare major.minor.patch only
            if current.major != latest.major
                || current.minor != latest.minor
                || current.patch != latest.patch
            {
                // Different version numbers - use normal semver comparison
                latest > current
            } else {
                // Same major.minor.patch but different pre-release/build metadata
                // Always offer to update in this case (e.g., 0.0.8 -> 0.0.8-1, 0.0.8-1 -> 0.0.8-2)
                // This handles hotfix releases properly
                true
            }
        }
        _ => {
            // Failed to parse one or both versions - version strings differ, so ask user
            info!("Cannot compare versions using semver, will prompt user");
            true
        }
    };

    if !needs_update {
        info!(
            "Current version ({}) is newer than or equal to latest ({})",
            version, latest_version
        );
        return Ok(());
    }

    info!(
        "Version difference detected: {} -> {}",
        version, latest_version
    );

    // Confirm update if not skipped
    if !skip_confirm {
        println!(
            "\nA different version is available: {} -> {}",
            version, latest_version
        );
        println!("Do you want to update? (y/N): ");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            info!("Update cancelled by user");
            return Ok(());
        }
    }

    info!("Downloading and installing update...");
    let status = if let Some(ref token) = github_token {
        self_update::backends::github::Update::configure()
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(version)
            .auth_token(token)
            .build()?
            .update()?
    } else {
        self_update::backends::github::Update::configure()
            .repo_owner(repo_owner)
            .repo_name(repo_name)
            .bin_name("snake")
            .show_download_progress(true)
            .current_version(version)
            .build()?
            .update()?
    };

    info!("Successfully updated to version: {}", status.version());
    println!("\n✓ Update successful! New version: {}", status.version());
    println!("Please restart the application to use the new version.");

    Ok(())
}
