use std::path::{Path, PathBuf};
use log::warn;
use crate::error::Result;

/// RAII wrapper for temporary files that ensures cleanup on drop
#[derive(Debug)]
pub struct TempFile {
    path: PathBuf,
    cleanup_on_drop: bool,
}

impl TempFile {
    /// Create a new TempFile from an existing path
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cleanup_on_drop: true,
        }
    }

    /// Get the path to the temporary file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Take ownership of the path and disable cleanup
    pub fn take_path(mut self) -> PathBuf {
        self.cleanup_on_drop = false;
        self.path.clone()
    }

    /// Manually cleanup the file (consumes self)
    pub fn cleanup(mut self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path)
                .map_err(|e| crate::error::fs_error(e, self.path.clone()))?;
        }
        self.cleanup_on_drop = false;
        Ok(())
    }

    /// Check if the file exists
    pub fn exists(&self) -> bool {
        self.path.exists()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if self.cleanup_on_drop && self.path.exists() {
            if let Err(e) = std::fs::remove_file(&self.path) {
                warn!("Failed to cleanup temporary file {:?}: {}", self.path, e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_temp_file_cleanup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        
        // Create file
        File::create(&file_path).unwrap();
        assert!(file_path.exists());
        
        // Wrap in TempFile
        {
            let _temp_file = TempFile::new(file_path.clone());
            assert!(file_path.exists());
        } // TempFile dropped here
        
        // File should be cleaned up
        assert!(!file_path.exists());
    }

    #[test]
    fn test_temp_file_take_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        
        File::create(&file_path).unwrap();
        
        let temp_file = TempFile::new(file_path.clone());
        let taken_path = temp_file.take_path();
        
        assert_eq!(taken_path, file_path);
        assert!(file_path.exists()); // Should still exist after take_path
    }
}