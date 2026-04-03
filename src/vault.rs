use anyhow::{Context, Result};
use std::path::PathBuf;

use crate::manifest::Manifest;

pub struct Vault {
    pub root: PathBuf,
}

impl Vault {
    pub fn new(root: PathBuf) -> Self {
        Vault { root }
    }

    pub fn raw_dir(&self) -> PathBuf {
        self.root.join("raw")
    }

    pub fn wiki_dir(&self) -> PathBuf {
        self.root.join("wiki")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root.join("state")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn prompts_dir(&self) -> PathBuf {
        self.root.join("prompts")
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.state_dir().join("manifest.json")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.root.join(".autovault.lock")
    }

    pub fn init(&self) -> Result<()> {
        for dir in [self.raw_dir(), self.wiki_dir(), self.state_dir(), self.logs_dir()] {
            std::fs::create_dir_all(&dir)
                .with_context(|| format!("creating directory {}", dir.display()))?;
        }

        let manifest_path = self.manifest_path();
        if !manifest_path.exists() {
            let manifest = Manifest::new();
            manifest.save(&manifest_path)?;
        }

        Ok(())
    }

    pub fn ensure_initialized(&self) -> Result<()> {
        if !self.state_dir().exists() {
            anyhow::bail!(
                "Vault not initialized at {}. Run `autovault init` first.",
                self.root.display()
            );
        }
        Ok(())
    }

    pub fn load_manifest(&self) -> Result<Manifest> {
        Manifest::load(&self.manifest_path())
    }

    pub fn save_manifest(&self, manifest: &Manifest) -> Result<()> {
        manifest.save(&self.manifest_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn init_creates_directories_and_manifest() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::new(dir.path().to_path_buf());
        vault.init().unwrap();

        assert!(vault.raw_dir().exists());
        assert!(vault.wiki_dir().exists());
        assert!(vault.state_dir().exists());
        assert!(vault.logs_dir().exists());
        assert!(vault.manifest_path().exists());

        let manifest = vault.load_manifest().unwrap();
        assert_eq!(manifest.version, "1.0");
    }

    #[test]
    fn init_is_idempotent() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::new(dir.path().to_path_buf());
        vault.init().unwrap();
        vault.init().unwrap(); // should not fail
    }

    #[test]
    fn ensure_initialized_fails_when_not_init() {
        let dir = TempDir::new().unwrap();
        let vault = Vault::new(dir.path().to_path_buf());
        assert!(vault.ensure_initialized().is_err());
    }

    #[test]
    fn path_helpers() {
        let vault = Vault::new(PathBuf::from("/tmp/v"));
        assert_eq!(vault.raw_dir(), PathBuf::from("/tmp/v/raw"));
        assert_eq!(vault.wiki_dir(), PathBuf::from("/tmp/v/wiki"));
        assert_eq!(vault.state_dir(), PathBuf::from("/tmp/v/state"));
        assert_eq!(vault.logs_dir(), PathBuf::from("/tmp/v/logs"));
        assert_eq!(vault.manifest_path(), PathBuf::from("/tmp/v/state/manifest.json"));
        assert_eq!(vault.lock_path(), PathBuf::from("/tmp/v/.autovault.lock"));
    }
}
