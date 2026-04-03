use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::path::Path;

pub fn file_hash(path: &Path) -> Result<String> {
    let data = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let hash = Sha256::digest(&data);
    Ok(format!("{:x}", hash))
}

pub fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)
        .context("creating temp file")?;
    tmp.write_all(content).context("writing temp file")?;
    tmp.persist(path)
        .with_context(|| format!("persisting to {}", path.display()))?;
    Ok(())
}

pub fn acquire_lock(lock_path: &Path) -> Result<()> {
    if lock_path.exists() {
        anyhow::bail!("Lock file exists: {}. Another instance may be running.", lock_path.display());
    }
    fs::write(lock_path, std::process::id().to_string())
        .with_context(|| format!("creating lock {}", lock_path.display()))?;
    Ok(())
}

pub fn release_lock(lock_path: &Path) -> Result<()> {
    if lock_path.exists() {
        fs::remove_file(lock_path)
            .with_context(|| format!("removing lock {}", lock_path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_file_hash() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.txt");
        fs::write(&path, "hello world").unwrap();
        let hash = file_hash(&path).unwrap();
        assert_eq!(
            hash,
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
    }

    #[test]
    fn test_atomic_write() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("out.txt");
        atomic_write(&path, b"content here").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "content here");
    }

    #[test]
    fn test_lock_acquire_release() {
        let dir = TempDir::new().unwrap();
        let lock = dir.path().join(".autovault.lock");
        acquire_lock(&lock).unwrap();
        assert!(lock.exists());
        // double acquire should fail
        assert!(acquire_lock(&lock).is_err());
        release_lock(&lock).unwrap();
        assert!(!lock.exists());
    }
}
