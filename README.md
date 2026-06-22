# CargoDrop

**CargoDrop** is a 100% portable, zero-host-pollution, self-contained Rust orchestration CLI. 

It is designed to let you drop a single compiled binary into a directory (e.g. on a USB stick or a locked-down machine) and compile Rust code without relying on a globally installed Rust toolchain, system environment variables, or host OS linkers.

---

## 📁 isolated Directory Layout

All paths are resolved dynamically at runtime relative only to the location of the `cargodrop` executable (`std::env::current_exe()`).

```
<current_exe_root>/
├── cargodrop (or cargodrop.exe)      <-- This binary
└── .cargodrop_sysroot/               <-- Hidden sysroot to keep the working dir clean
    ├── .cargo_cache/                 <-- Spoofed CARGO_HOME (registry, git checkouts)
    ├── .rustup_cache/                <-- Spoofed RUSTUP_HOME
    ├── toolchain/
    │   ├── bin/                      <-- Contains rustc, cargo, rust-lld, rustup
    │   └── lib/                      <-- Contains rustlib (Standard library)
    └── .cargodrop_state.json         <-- Tracks currently installed version and setup status
```

---

## ✨ Features

*   **Zero Host Leakage**: Never touches `~/.cargo`, `~/.rustup`, or modifies the system `$PATH`. All caches and state remain entirely within `.cargodrop_sysroot/`.
*   **Windows UNC Path Defense**: Strips the problematic `\\?\` and `\\?\UNC\` path prefixes on Windows to prevent `rustc` and `cargo` from failing during path resolution.
*   **Transparent Proxying**: Passes stdout, stderr, stdin, ANSI colors, and exit codes directly to the terminal.
*   **Graceful Signal Forwarding**: Gracefully intercepts Ctrl+C and propagates it to the active `cargo` child process, allowing compilation databases to clean up before exit.
*   **Rustup Integration**: Bundles and configures an isolated `rustup` instance linked to the self-contained toolchain so you can inspect compiler settings.
*   **Crate & Target Support**: Full support for compiling crates.io dependencies and cross-compiling to other architectures (like WebAssembly) using custom target injection.

---

## 🚀 Getting Started

### 1. Build the Binary
Clone this repository and compile the CargoDrop binary:
```bash
cargo build --release
```
Take the output executable `target/release/cargodrop` (or `cargodrop.exe`) and place it in any folder where you wish to set up your portable dev environment.

### 2. Bootstrap the Environment
Initialize the isolated environment. By default, this downloads the latest stable compiler, cargo, target standard library, and configures the isolated database:
```bash
cargodrop setup
```
You can also specify a specific Rust version or channel:
```bash
cargodrop setup 1.80.0
cargodrop setup nightly
```

---

## 🛠️ Usage

### 📦 Proxying Cargo Commands
Compile and manage your projects exactly like using native cargo. All downloads and registry caches are isolated inside `.cargodrop_sysroot/`:
```bash
cargodrop cargo init my_project
cargodrop cargo build --manifest-path my_project/Cargo.toml
cargodrop cargo run --manifest-path my_project/Cargo.toml
```

### ⚙️ Proxying Rustup Commands
Manage the isolated environment's toolchain configuration:
```bash
cargodrop rustup show
```

### 🎯 Adding Cross-Compilation Targets
Because linked toolchains are read-only to `rustup`, CargoDrop provides a custom command to download and extract standard libraries for additional targets (e.g. WebAssembly):
```bash
cargodrop target add wasm32-unknown-unknown
```
Once added, you can compile targeting WebAssembly:
```bash
cargodrop cargo build --target wasm32-unknown-unknown
```

### 🧹 Reclaiming Disk Space
Wipe the entire `.cargodrop_sysroot/` directory structure to free up disk space:
```bash
cargodrop clean
```

---

## 🔍 Technical Details (How it Works)

When proxying commands, CargoDrop spawns a child process and injects a clean environment block:
*   `CARGO_HOME = <root>/.cargodrop_sysroot/.cargo_cache`
*   `RUSTUP_HOME = <root>/.cargodrop_sysroot/.rustup_cache`
*   `RUSTC = <root>/.cargodrop_sysroot/toolchain/bin/rustc`
*   `PATH`: Prepends `<root>/.cargodrop_sysroot/toolchain/bin` to the original `$PATH` so the bundled tools take priority.
*   `CARGO_ENCODED_RUSTFLAGS`: Appends `-C linker=rust-lld` to force compilation using LLVM's LLD linker, eliminating the dependency on MSVC (`link.exe`) or GCC (`ld`) on the host system.

### 💾 Portability & Offline Compilation
Because all registry files, toolchains, and crates are stored relative to the executable under `.cargodrop_sysroot/`, you can:
1. Run `cargodrop setup` and compile your project once on an **online** machine.
2. Copy the entire parent directory onto a USB stick.
3. Plug the stick into a **completely offline, locked-down** machine.
4. Run compilation offline by appending the offline flag:
   ```bash
   cargodrop cargo build --offline
   ```

---

## License

GNU General Public License v3.0
