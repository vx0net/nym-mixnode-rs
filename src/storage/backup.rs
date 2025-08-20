// Backup and restore functionality
use std::path::PathBuf;
use std::time::SystemTime;
use serde::{Serialize, Deserialize};
use tracing::{info, error};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupConfig {
    pub backup_dir: PathBuf,
    pub max_backups: usize,
    pub compression: bool,
}

pub struct BackupManager {
    config: BackupConfig,
}

impl BackupManager {
    pub fn new(config: BackupConfig) -> Result<Self, String> {
        std::fs::create_dir_all(&config.backup_dir)
            .map_err(|e| format!("Failed to create backup directory: {}", e))?;
        
        Ok(Self { config })
    }
    
    pub async fn create_backup(&self, data_dir: &PathBuf) -> Result<PathBuf, String> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        
        let backup_name = format!("backup-{}.tar", timestamp);
        let backup_path = self.config.backup_dir.join(&backup_name);
        
        // Simple file copy for demonstration
        // In production, use proper archiving
        info!("Creating backup: {:?}", backup_path);
        
        // Copy data directory to backup location
        if let Err(e) = self.copy_directory(data_dir, &backup_path) {
            error!("Backup failed: {}", e);
            return Err(format!("Backup failed: {}", e));
        }
        
        self.cleanup_old_backups().await?;
        
        info!("Backup created successfully: {:?}", backup_path);
        Ok(backup_path)
    }
    
    pub async fn restore_backup(&self, backup_path: &PathBuf, target_dir: &PathBuf) -> Result<(), String> {
        info!("Restoring backup from: {:?}", backup_path);
        
        if !backup_path.exists() {
            return Err("Backup file does not exist".to_string());
        }
        
        // Simple file copy for demonstration
        self.copy_directory(backup_path, target_dir)
            .map_err(|e| format!("Restore failed: {}", e))?;
        
        info!("Backup restored successfully to: {:?}", target_dir);
        Ok(())
    }
    
    pub async fn list_backups(&self) -> Result<Vec<PathBuf>, String> {
        let mut backups = Vec::new();
        
        let entries = std::fs::read_dir(&self.config.backup_dir)
            .map_err(|e| format!("Failed to read backup directory: {}", e))?;
        
        for entry in entries {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();
            
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("tar")) {
                backups.push(path);
            }
        }
        
        backups.sort();
        Ok(backups)
    }
    
    async fn cleanup_old_backups(&self) -> Result<(), String> {
        let backups = self.list_backups().await?;
        
        if backups.len() > self.config.max_backups {
            let to_remove = backups.len() - self.config.max_backups;
            
            for backup in backups.iter().take(to_remove) {
                if let Err(e) = std::fs::remove_file(backup) {
                    error!("Failed to remove old backup {:?}: {}", backup, e);
                } else {
                    info!("Removed old backup: {:?}", backup);
                }
            }
        }
        
        Ok(())
    }
    
    fn copy_directory(&self, src: &PathBuf, dst: &PathBuf) -> Result<(), std::io::Error> {
        // Simple implementation - in production use proper archiving library
        if src.is_file() {
            std::fs::copy(src, dst)?;
        } else if src.is_dir() {
            std::fs::create_dir_all(dst)?;
            for entry in std::fs::read_dir(src)? {
                let entry = entry?;
                let src_path = entry.path();
                let dst_path = dst.join(entry.file_name());
                self.copy_directory(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }
}
