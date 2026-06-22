use std::path::{Path, PathBuf};
use anyhow::Result;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CargoDropConfig {
    pub root_dir: PathBuf,
    pub sysroot_dir: PathBuf,
    pub cargo_home: PathBuf,
    pub rustup_home: PathBuf,
    pub toolchain_dir: PathBuf,
    pub bin_dir: PathBuf,
    pub lib_dir: PathBuf,
    pub state_file: PathBuf,
}

impl CargoDropConfig {
    pub fn new() -> Result<Self> {
        let exe_path = std::env::current_exe()?;
        let raw_parent = exe_path.parent()
            .ok_or_else(|| anyhow::anyhow!("Failed to find current executable parent directory"))?;
        
        // Canonicalize the parent directory to resolve any relative components/symlinks
        let root_dir = std::fs::canonicalize(raw_parent)?;
        // Sanitize the path to strip Windows UNC prefix \\?\
        let root_dir = sanitize_path(&root_dir);
        
        let sysroot_dir = root_dir.join(".cargodrop_sysroot");
        let cargo_home = sysroot_dir.join(".cargo_cache");
        let rustup_home = sysroot_dir.join(".rustup_cache");
        let toolchain_dir = sysroot_dir.join("toolchain");
        let bin_dir = toolchain_dir.join("bin");
        let lib_dir = toolchain_dir.join("lib");
        let state_file = sysroot_dir.join(".cargodrop_state.json");
        
        Ok(Self {
            root_dir,
            sysroot_dir,
            cargo_home,
            rustup_home,
            toolchain_dir,
            bin_dir,
            lib_dir,
            state_file,
        })
    }

    pub fn ensure_dirs(&self) -> Result<()> {
        std::fs::create_dir_all(&self.sysroot_dir)?;
        std::fs::create_dir_all(&self.cargo_home)?;
        std::fs::create_dir_all(&self.rustup_home)?;
        std::fs::create_dir_all(&self.toolchain_dir)?;
        std::fs::create_dir_all(&self.bin_dir)?;
        std::fs::create_dir_all(&self.lib_dir)?;
        Ok(())
    }
}

/// Sanitizes a path, stripping the Windows UNC prefix `\\?\` if present.
/// On Windows, `std::fs::canonicalize` returns UNC paths which cargo/rustc
/// can fail to parse properly.
pub fn sanitize_path(path: &Path) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let path_str = path.to_string_lossy();
        if path_str.starts_with(r"\\?\UNC\") {
            // e.g. \\?\UNC\server\share -> \\server\share
            PathBuf::from(format!(r"\\{}", &path_str[8..]))
        } else if path_str.starts_with(r"\\?\") {
            // e.g. \\?\C:\foo -> C:\foo
            PathBuf::from(&path_str[4..])
        } else {
            path.to_path_buf()
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        path.to_path_buf()
    }
}
