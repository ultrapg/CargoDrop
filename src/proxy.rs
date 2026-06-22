use std::process::Command;
use crate::context::CargoDropConfig;
use anyhow::{Result, Context};

pub fn run_cargo_proxy(config: &CargoDropConfig, cargo_args: &[String]) -> Result<i32> {
    let cargo_bin = crate::context::sanitize_path(&config.bin_dir.join(if cfg!(target_os = "windows") { "cargo.exe" } else { "cargo" }));
    if !cargo_bin.exists() {
        anyhow::bail!("Cargo binary not found at {:?}", cargo_bin);
    }
    
    let cargo_home = crate::context::sanitize_path(&config.cargo_home);
    let rustup_home = crate::context::sanitize_path(&config.rustup_home);
    let rustc_path = crate::context::sanitize_path(&config.bin_dir.join(if cfg!(target_os = "windows") { "rustc.exe" } else { "rustc" }));
    
    // Prep path
    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = std::env::split_paths(&original_path).collect::<Vec<_>>();
    // Prepend the toolchain's bin directory
    paths.insert(0, config.bin_dir.clone());
    let new_path = std::env::join_paths(paths).context("Failed to reconstruct PATH variable")?;
    
    // Inject Linker Mandate flags (-C linker=rust-lld)
    let mut rustflags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    if !rustflags.is_empty() {
        rustflags.push('\x1f');
    }
    rustflags.push_str("-C\x1flinker=rust-lld");
    
    let mut cmd = Command::new(&cargo_bin);
    cmd.args(cargo_args)
        .env("CARGO_HOME", cargo_home)
        .env("RUSTUP_HOME", rustup_home)
        .env("RUSTC", rustc_path)
        .env("PATH", new_path)
        .env("CARGO_ENCODED_RUSTFLAGS", rustflags)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
        
    // Register Ctrl+C handler so the parent waits for Cargo child to clean up and exit
    let ctrlc_handled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ctrlc_handled_clone = ctrlc_handled.clone();
    
    if let Err(e) = ctrlc::set_handler(move || {
        ctrlc_handled_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    }) {
        // Log error but don't crash if handler cannot be set (e.g. non-TTY environments)
        eprintln!("Warning: Failed to set Ctrl+C handler: {}", e);
    }
    
    let mut child = cmd.spawn().context("Failed to spawn Cargo process")?;
    let status = child.wait().context("Failed to wait for Cargo child process")?;
    
    let exit_code = status.code().unwrap_or_else(|| {
        if ctrlc_handled.load(std::sync::atomic::Ordering::SeqCst) {
            130 // Standard exit code for SIGINT (128 + 2)
        } else {
            -1
        }
    });
    
    Ok(exit_code)
}

pub fn run_rustup_proxy(config: &CargoDropConfig, rustup_args: &[String]) -> Result<i32> {
    let rustup_bin = crate::context::sanitize_path(&config.bin_dir.join(if cfg!(target_os = "windows") { "rustup.exe" } else { "rustup" }));
    if !rustup_bin.exists() {
        anyhow::bail!("Rustup binary not found at {:?}. Please run setup first.", rustup_bin);
    }
    
    let cargo_home = crate::context::sanitize_path(&config.cargo_home);
    let rustup_home = crate::context::sanitize_path(&config.rustup_home);
    
    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = std::env::split_paths(&original_path).collect::<Vec<_>>();
    paths.insert(0, config.bin_dir.clone());
    let new_path = std::env::join_paths(paths).context("Failed to reconstruct PATH variable")?;
    
    let mut cmd = Command::new(&rustup_bin);
    cmd.args(rustup_args)
        .env("CARGO_HOME", cargo_home)
        .env("RUSTUP_HOME", rustup_home)
        .env("PATH", new_path)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
        
    let ctrlc_handled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ctrlc_handled_clone = ctrlc_handled.clone();
    
    if let Err(e) = ctrlc::set_handler(move || {
        ctrlc_handled_clone.store(true, std::sync::atomic::Ordering::SeqCst);
    }) {
        eprintln!("Warning: Failed to set Ctrl+C handler: {}", e);
    }
    
    let mut child = cmd.spawn().context("Failed to spawn Rustup process")?;
    let status = child.wait().context("Failed to wait for Rustup child process")?;
    
    let exit_code = status.code().unwrap_or_else(|| {
        if ctrlc_handled.load(std::sync::atomic::Ordering::SeqCst) {
            130
        } else {
            -1
        }
    });
    
    Ok(exit_code)
}

