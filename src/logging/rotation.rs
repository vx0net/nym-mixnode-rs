// Log rotation functionality
use std::path::PathBuf;
use std::time::SystemTime;
use super::RotationConfig;

pub struct LogRotator {
    config: RotationConfig,
    current_file: PathBuf,
}

impl LogRotator {
    pub fn new(config: RotationConfig, file_path: PathBuf) -> Self {
        Self {
            config,
            current_file: file_path,
        }
    }
    
    pub fn should_rotate(&self) -> bool {
        match &self.config {
            RotationConfig::Size(max_mb) => {
                if let Ok(metadata) = std::fs::metadata(&self.current_file) {
                    let size_mb = metadata.len() / (1024 * 1024);
                    size_mb >= *max_mb
                } else {
                    false
                }
            },
            RotationConfig::Time(_) => {
                // Simplified time-based rotation
                false
            },
            RotationConfig::Both(max_mb, _) => {
                if let Ok(metadata) = std::fs::metadata(&self.current_file) {
                    let size_mb = metadata.len() / (1024 * 1024);
                    size_mb >= *max_mb
                } else {
                    false
                }
            }
        }
    }
    
    pub fn rotate(&self) -> Result<PathBuf, std::io::Error> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let rotated_name = format!("{}.{}", 
            self.current_file.display(), 
            timestamp
        );
        let rotated_path = PathBuf::from(rotated_name);
        
        std::fs::rename(&self.current_file, &rotated_path)?;
        
        Ok(rotated_path)
    }
}
