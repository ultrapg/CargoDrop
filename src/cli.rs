use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "cargodrop")]
#[command(author = "DeepMind Antigravity Team")]
#[command(version = "0.1.0")]
#[command(about = "CargoDrop: A 100% portable, zero-host-pollution, self-contained Rust orchestration CLI.", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Bootstraps and configures the self-contained Rust toolchain
    Setup {
        /// The Rust channel (stable, beta, nightly) or exact version (e.g. 1.80.0)
        version: Option<String>,
    },
    /// Proxies commands to the self-contained Cargo binary
    Cargo {
        /// Arguments forwarded directly to the bundled Cargo executable
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Proxies commands to the self-contained Rustup binary
    Rustup {
        /// Arguments forwarded directly to the bundled Rustup executable
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Manage target architectures for the isolated toolchain
    Target {
        #[command(subcommand)]
        subcommand: TargetSubcommands,
    },
    /// Clean and remove the .cargodrop_sysroot environment to reclaim disk space
    Clean,
}

#[derive(Subcommand, Debug)]
pub enum TargetSubcommands {
    /// Add a new compilation target (e.g. wasm32-unknown-unknown)
    Add {
        /// The target triple to add (e.g. wasm32-unknown-unknown)
        target: String,
    },
}
