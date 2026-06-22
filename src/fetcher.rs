use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Result};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use flate2::read::GzDecoder;
use tar::Archive;

use crate::context::CargoDropConfig;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CargoDropState {
    pub version: String,
    pub target: String,
    pub status: String,
    pub installed_at: String,
}

impl CargoDropState {
    pub fn read_state(state_file: &Path) -> Result<Option<Self>> {
        if !state_file.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(state_file)?;
        let state = serde_json::from_str(&content)?;
        Ok(Some(state))
    }

    pub fn write_state(&self, state_file: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)?;
        fs::write(state_file, content)?;
        Ok(())
    }
}

pub fn detect_host_target() -> Result<String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "aarch64" => "aarch64",
        "i686" => "i686",
        other => anyhow::bail!("Unsupported CPU architecture: {}", other),
    };
    let os = match std::env::consts::OS {
        "windows" => "pc-windows-msvc",
        "linux" => "unknown-linux-gnu",
        "macos" => "apple-darwin",
        other => anyhow::bail!("Unsupported operating system: {}", other),
    };
    Ok(format!("{}-{}", arch, os))
}

fn verify_sha256(file_path: &Path, expected_hash: &str) -> Result<()> {
    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    let result = hasher.finalize();
    let hex_hash = format!("{:x}", result);
    if hex_hash != expected_hash {
        anyhow::bail!(
            "Checksum verification failed. Expected: {}, got: {}",
            expected_hash,
            hex_hash
        );
    }
    Ok(())
}

pub fn download_file(
    client: &reqwest::blocking::Client,
    url: &str,
    dest_path: &Path,
    expected_hash: &str,
    component_name: &str,
) -> Result<()> {
    println!("Downloading {}...", component_name.cyan());
    let mut response = client.get(url).send()?;
    if !response.status().is_success() {
        anyhow::bail!("Failed to download {}: HTTP {}", component_name, response.status());
    }

    let total_size = response.content_length().unwrap_or(0);
    let pb = if total_size > 0 {
        ProgressBar::new(total_size)
    } else {
        ProgressBar::new_spinner()
    };

    pb.set_style(ProgressStyle::with_template(
        "[{spinner:.green}] [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta}) {msg}"
    ).unwrap().progress_chars("#>-"));
    pb.set_message(component_name.to_string());

    let mut dest_file = File::create(dest_path)?;
    let mut buffer = [0; 8192];
    let mut downloaded = 0;

    loop {
        let n = response.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        dest_file.write_all(&buffer[..n])?;
        downloaded += n as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete");

    println!("Verifying checksum for {}...", component_name.cyan());
    verify_sha256(dest_path, expected_hash)?;
    println!("Checksum verified successfully.");
    Ok(())
}

pub fn extract_tar_gz(
    archive_path: &Path,
    dest_bin_dir: &Path,
    dest_lib_dir: &Path,
    component_name: &str,
    target: &str,
) -> Result<()> {
    let file = File::open(archive_path)?;
    let metadata = file.metadata()?;
    let file_len = metadata.len();
    
    let pb = ProgressBar::new(file_len);
    pb.set_style(ProgressStyle::with_template(
        &format!("[{{spinner:.green}}] Extracting {}: [{{elapsed_precise}}] [{{bar:40.cyan/blue}}] {{bytes}}/{{total_bytes}} ({{eta}})", component_name)
    ).unwrap().progress_chars("#>-"));
    
    let pb_reader = pb.wrap_read(file);
    let tar = GzDecoder::new(pb_reader);
    let mut archive = Archive::new(tar);
    
    for entry_res in archive.entries()? {
        let mut entry = entry_res?;
        let path = entry.path()?.to_path_buf();
        
        let components: Vec<_> = path.components()
            .map(|c| c.as_os_str().to_string_lossy().to_string())
            .collect();
            
        if components.len() < 3 {
            continue;
        }
        
        let sub_component = &components[1];
        let category = &components[2];
        
        let matches = match component_name {
            "rustc" => sub_component == "rustc" && (category == "bin" || category == "lib"),
            "cargo" => sub_component == "cargo" && category == "bin",
            "rust-std" => sub_component == &format!("rust-std-{}", target) && category == "lib",
            _ => false,
        };
        
        if matches {
            let dest_base = if category == "bin" {
                dest_bin_dir
            } else {
                dest_lib_dir
            };
            
            let relative_path: PathBuf = components[3..].iter().collect();
            let dest_path = dest_base.join(relative_path);
            
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }
            
            entry.unpack(&dest_path)?;
        }
    }
    
    pb.finish_with_message("Extraction complete");
    Ok(())
}

fn get_component_info(manifest: &toml::Value, pkg: &str, target: &str) -> Result<(String, String)> {
    let pkg_table = manifest.get("pkg")
        .and_then(|v| v.get(pkg))
        .ok_or_else(|| anyhow!("Package '{}' not found in manifest", pkg))?;
    
    let target_table = pkg_table.get("target")
        .and_then(|v| v.get(target))
        .ok_or_else(|| anyhow!("Target '{}' for package '{}' not found in manifest", target, pkg))?;
    
    let available = target_table.get("available")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    
    if !available {
        anyhow::bail!("Package '{}' for target '{}' is not available in this release", pkg, target);
    }
    
    let url = target_table.get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing url for package '{}' target '{}'", pkg, target))?
        .to_string();
        
    let hash = target_table.get("hash")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("Missing hash for package '{}' target '{}'", pkg, target))?
        .to_string();
        
    Ok((url, hash))
}

pub fn setup_toolchain(config: &CargoDropConfig, version: Option<String>) -> Result<()> {
    let target = detect_host_target()?;
    let version_str = version.unwrap_or_else(|| "stable".to_string());
    println!("Initializing toolchain setup for target: {} (version: {})", target.green(), version_str.green());
    
    let manifest_url = format!("https://static.rust-lang.org/dist/channel-rust-{}.toml", version_str);
    println!("Fetching manifest from {}...", manifest_url.cyan());
    
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargodrop")
        .build()?;
        
    let response = client.get(&manifest_url).send()?;
    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch manifest: HTTP {}", response.status());
    }
    let manifest_text = response.text()?;
    let manifest: toml::Value = toml::from_str(&manifest_text)?;
    
    // Read exact release version date
    let manifest_date = manifest.get("date")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    println!("Manifest date: {}", manifest_date.green());
    
    // Resolve URLs & hashes
    let (rustc_url, rustc_hash) = get_component_info(&manifest, "rustc", &target)?;
    let (cargo_url, cargo_hash) = get_component_info(&manifest, "cargo", &target)?;
    let (rust_std_url, rust_std_hash) = get_component_info(&manifest, "rust-std", &target)?;
    
    // Recreate clean toolchain directories
    if config.toolchain_dir.exists() {
        println!("Cleaning up existing toolchain directory...");
        let _ = fs::remove_dir_all(&config.toolchain_dir);
    }
    config.ensure_dirs()?;
    
    // Set up temp download directory
    let temp_dir = config.sysroot_dir.join("temp_downloads");
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
    }
    fs::create_dir_all(&temp_dir)?;
    
    let rustc_archive = temp_dir.join("rustc.tar.gz");
    let cargo_archive = temp_dir.join("cargo.tar.gz");
    let rust_std_archive = temp_dir.join("rust-std.tar.gz");
    
    // Download and extract in order
    download_file(&client, &rustc_url, &rustc_archive, &rustc_hash, "rustc")?;
    extract_tar_gz(&rustc_archive, &config.bin_dir, &config.lib_dir, "rustc", &target)?;
    
    download_file(&client, &cargo_url, &cargo_archive, &cargo_hash, "cargo")?;
    extract_tar_gz(&cargo_archive, &config.bin_dir, &config.lib_dir, "cargo", &target)?;
    
    download_file(&client, &rust_std_url, &rust_std_archive, &rust_std_hash, "rust-std")?;
    extract_tar_gz(&rust_std_archive, &config.bin_dir, &config.lib_dir, "rust-std", &target)?;
    
    // Handle rust-lld copy
    let lld_filename = if cfg!(target_os = "windows") { "rust-lld.exe" } else { "rust-lld" };
    let source_lld = config.lib_dir.join("rustlib").join(&target).join("bin").join(lld_filename);
    let dest_lld = config.bin_dir.join(lld_filename);
    
    if source_lld.exists() {
        println!("Found bundled rust-lld. Copying to bin directory...");
        fs::copy(&source_lld, &dest_lld)?;
    } else {
        println!("Warning: Bundled rust-lld not found at {:?}", source_lld);
    }
    
    // Bootstrapping Rustup integration
    if let Err(e) = bootstrap_rustup(config, &target) {
        println!("Warning: Failed to bootstrap rustup integration: {}", e);
    }
    
    // Clean up temporary archives
    println!("Cleaning up download files...");
    let _ = fs::remove_dir_all(&temp_dir);
    
    // Write setup state
    let state = CargoDropState {
        version: version_str,
        target,
        status: "valid".to_string(),
        installed_at: chrono::Local::now().to_rfc3339(),
    };
    state.write_state(&config.state_file)?;
    
    println!("{}", "CargoDrop environment successfully bootstrapped and ready!".green().bold());
    Ok(())
}

pub fn bootstrap_rustup(config: &CargoDropConfig, target: &str) -> Result<()> {
    println!("Bootstrapping Rustup integration...");
    
    let suffix = if cfg!(target_os = "windows") { ".exe" } else { "" };
    let rustup_init_url = format!(
        "https://static.rust-lang.org/rustup/dist/{}/rustup-init{}",
        target, suffix
    );
    
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargodrop")
        .build()?;
        
    let temp_dir = config.sysroot_dir.join("temp_rustup_setup");
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
    }
    fs::create_dir_all(&temp_dir)?;
    
    let rustup_init_path = temp_dir.join(format!("rustup-init{}", suffix));
    
    println!("Downloading rustup-init from {}...", rustup_init_url.cyan());
    let mut response = client.get(&rustup_init_url).send()?;
    if !response.status().is_success() {
        anyhow::bail!("Failed to download rustup-init: HTTP {}", response.status());
    }
    
    let mut dest_file = File::create(&rustup_init_path)?;
    std::io::copy(&mut response, &mut dest_file)?;
    drop(dest_file);
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&rustup_init_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&rustup_init_path, perms)?;
    }
    
    println!("Initializing rustup database silently...");
    let cargo_home_sanitized = crate::context::sanitize_path(&config.cargo_home);
    let rustup_home_sanitized = crate::context::sanitize_path(&config.rustup_home);
    
    let mut cmd = std::process::Command::new(&rustup_init_path);
    cmd.args(&["-y", "--no-modify-path", "--default-toolchain", "none"])
        .env("CARGO_HOME", &cargo_home_sanitized)
        .env("RUSTUP_HOME", &rustup_home_sanitized)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
        
    let status = cmd.status()?;
    if !status.success() {
        anyhow::bail!("Failed to run rustup-init installer");
    }
    
    // Copy the rustup binary from CARGO_HOME/bin/rustup to toolchain/bin/rustup
    let installed_rustup = config.cargo_home.join("bin").join(format!("rustup{}", suffix));
    let dest_rustup = config.bin_dir.join(format!("rustup{}", suffix));
    
    if installed_rustup.exists() {
        fs::copy(&installed_rustup, &dest_rustup)?;
        println!("Rustup binary placed in toolchain bin directory.");
    } else {
        // Fallback: copy rustup-init directly
        fs::copy(&rustup_init_path, &dest_rustup)?;
        println!("Rustup binary (renamed rustup-init) placed in toolchain bin directory.");
    }
    
    // Link our manually installed toolchain as 'stable'
    println!("Linking self-contained toolchain inside rustup...");
    let toolchain_dir_sanitized = crate::context::sanitize_path(&config.toolchain_dir);
    
    let mut cmd = std::process::Command::new(&dest_rustup);
    cmd.args(&["toolchain", "link", "stable", &toolchain_dir_sanitized.to_string_lossy()])
        .env("CARGO_HOME", &cargo_home_sanitized)
        .env("RUSTUP_HOME", &rustup_home_sanitized)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = cmd.status();
    
    let mut cmd = std::process::Command::new(&dest_rustup);
    cmd.args(&["default", "stable"])
        .env("CARGO_HOME", &cargo_home_sanitized)
        .env("RUSTUP_HOME", &rustup_home_sanitized)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _ = cmd.status();
    
    let _ = fs::remove_dir_all(&temp_dir);
    println!("Rustup successfully configured.");
    Ok(())
}

pub fn setup_target(config: &CargoDropConfig, new_target: &str) -> Result<()> {
    let state = CargoDropState::read_state(&config.state_file)?
        .ok_or_else(|| anyhow!("CargoDrop is not bootstrapped. Run setup first."))?;
        
    println!("Adding target: {} (version: {})", new_target.green(), state.version.green());
    
    let manifest_url = format!("https://static.rust-lang.org/dist/channel-rust-{}.toml", state.version);
    println!("Fetching manifest from {}...", manifest_url.cyan());
    
    let client = reqwest::blocking::Client::builder()
        .user_agent("cargodrop")
        .build()?;
        
    let response = client.get(&manifest_url).send()?;
    if !response.status().is_success() {
        anyhow::bail!("Failed to fetch manifest: HTTP {}", response.status());
    }
    let manifest_text = response.text()?;
    let manifest: toml::Value = toml::from_str(&manifest_text)?;
    
    let (rust_std_url, rust_std_hash) = get_component_info(&manifest, "rust-std", new_target)?;
    
    let temp_dir = config.sysroot_dir.join("temp_target_downloads");
    if temp_dir.exists() {
        let _ = fs::remove_dir_all(&temp_dir);
    }
    fs::create_dir_all(&temp_dir)?;
    
    let rust_std_archive = temp_dir.join("rust-std.tar.gz");
    
    download_file(&client, &rust_std_url, &rust_std_archive, &rust_std_hash, &format!("rust-std for {}", new_target))?;
    extract_tar_gz(&rust_std_archive, &config.bin_dir, &config.lib_dir, "rust-std", new_target)?;
    
    let _ = fs::remove_dir_all(&temp_dir);
    println!("Target {} successfully added to the toolchain.", new_target.green().bold());
    Ok(())
}

