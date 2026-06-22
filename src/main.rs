use clap::Parser;
use colored::*;
use std::process;

mod cli;
mod context;
mod fetcher;
mod proxy;

use cli::{Cli, Commands, TargetSubcommands};
use context::CargoDropConfig;
use fetcher::CargoDropState;

fn main() {
    if let Err(err) = run() {
        eprintln!("{}: {}", "Error".red().bold(), err);
        process::exit(1);
    }
}

fn run() -> anyhow::Result<()> {
    let args = Cli::parse();
    let config = CargoDropConfig::new()?;
    
    match args.command {
        Commands::Setup { version } => {
            fetcher::setup_toolchain(&config, version)?;
        }
        Commands::Cargo { args } => {
            // Check if toolchain is bootstrapped
            let is_ready = if let Ok(Some(state)) = CargoDropState::read_state(&config.state_file) {
                state.status == "valid"
            } else {
                false
            };
            
            if !is_ready {
                anyhow::bail!(
                    "CargoDrop is not bootstrapped yet.\n\
                     Please run {} first to download and configure the isolated Rust toolchain.",
                    "cargodrop setup".green().bold()
                );
            }
            
            let exit_code = proxy::run_cargo_proxy(&config, &args)?;
            process::exit(exit_code);
        }
        Commands::Rustup { args } => {
            // Check if toolchain is bootstrapped
            let is_ready = if let Ok(Some(state)) = CargoDropState::read_state(&config.state_file) {
                state.status == "valid"
            } else {
                false
            };
            
            if !is_ready {
                anyhow::bail!(
                    "CargoDrop is not bootstrapped yet.\n\
                     Please run {} first to download and configure the isolated Rust toolchain.",
                    "cargodrop setup".green().bold()
                );
            }
            
            let exit_code = proxy::run_rustup_proxy(&config, &args)?;
            process::exit(exit_code);
        }
        Commands::Target { subcommand } => {
            match subcommand {
                TargetSubcommands::Add { target } => {
                    fetcher::setup_target(&config, &target)?;
                }
            }
        }
        Commands::Clean => {
            if config.sysroot_dir.exists() {
                println!("Removing sysroot at {:?}...", config.sysroot_dir);
                std::fs::remove_dir_all(&config.sysroot_dir)?;
                println!("{}", "Successfully wiped .cargodrop_sysroot/ directory.".green().bold());
            } else {
                println!("Nothing to clean. Sysroot directory does not exist.");
            }
        }
    }
    
    Ok(())
}
