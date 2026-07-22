use std::path::{Path, PathBuf};

use crate::error::{PulseError, Result};

/// Resolve the default Pulse data directory: `%LOCALAPPDATA%\Pulse` on Windows,
/// and the platform equivalent of local data dir + `Pulse` elsewhere.
pub fn default_data_dir() -> Result<PathBuf> {
    let base = directories::BaseDirs::new()
        .ok_or_else(|| PulseError::Config("could not resolve home/local data dirs".into()))?;
    Ok(base.data_local_dir().join("Pulse"))
}

/// Paths under a Pulse data root.
#[derive(Debug, Clone)]
pub struct PulsePaths {
    pub root: PathBuf,
}

impl PulsePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn default() -> Result<Self> {
        Ok(Self::new(default_data_dir()?))
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.join("pulse.db")
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn exports_dir(&self) -> PathBuf {
        self.root.join("exports")
    }

    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }

    pub fn service_pid_path(&self) -> PathBuf {
        self.root.join("service.pid")
    }

    /// Create root and common subdirs if missing.
    pub fn ensure_layout(&self) -> Result<()> {
        for dir in [
            self.root.as_path(),
            self.logs_dir().as_path(),
            self.exports_dir().as_path(),
            self.tmp_dir().as_path(),
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// Join and normalize; reject empty roots.
pub fn require_dir(path: &Path) -> Result<PathBuf> {
    if path.as_os_str().is_empty() {
        return Err(PulseError::Validation("data dir path is empty".into()));
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_data_dir_ends_with_pulse() {
        let dir = default_data_dir().expect("data dir");
        assert_eq!(
            dir.file_name().and_then(|s| s.to_str()),
            Some("Pulse")
        );
    }

    #[test]
    fn paths_under_root() {
        let p = PulsePaths::new(r"C:\tmp\PulseTest");
        assert!(p.db_path().ends_with("pulse.db"));
        assert!(p.config_path().ends_with("config.toml"));
        assert!(p.service_pid_path().ends_with("service.pid"));
    }
}
