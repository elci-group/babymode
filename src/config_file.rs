use crate::config::{ConfigBuilder, WhisperModel};
use crate::error::{BabymodeError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tokio::fs;

/// Configuration file format that can be serialized to YAML/JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    /// Default whisper model to use
    pub whisper_model: Option<String>,
    /// Default volume level during censoring
    pub censor_volume: Option<f32>,
    /// Default fade duration in seconds
    pub fade_duration: Option<f32>,
    /// Custom swear words list
    pub swear_words: Option<Vec<String>>,
    /// Default output directory
    pub output_directory: Option<PathBuf>,
    /// Enable progress indicators by default
    pub show_progress: Option<bool>,
    /// Language for processing (future enhancement)
    pub language: Option<String>,
    /// Custom profiles
    pub profiles: Option<std::collections::HashMap<String, ProfileConfig>>,
}

/// Profile-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    pub whisper_model: Option<String>,
    pub censor_volume: Option<f32>,
    pub fade_duration: Option<f32>,
    pub swear_words: Option<Vec<String>>,
    pub description: Option<String>,
}

impl Default for ConfigFile {
    fn default() -> Self {
        let mut profiles = std::collections::HashMap::new();
        
        // Add some default profiles
        profiles.insert("strict".to_string(), ProfileConfig {
            censor_volume: Some(0.0),
            fade_duration: Some(0.1),
            swear_words: Some(vec![
                "fuck".to_string(), "shit".to_string(), "damn".to_string(),
                "hell".to_string(), "ass".to_string(), "bitch".to_string(),
                "bastard".to_string(), "crap".to_string(), "piss".to_string(),
            ]),
            whisper_model: Some("base".to_string()),
            description: Some("Strict censoring with complete silence".to_string()),
        });
        
        profiles.insert("mild".to_string(), ProfileConfig {
            censor_volume: Some(0.3),
            fade_duration: Some(0.3),
            swear_words: Some(vec![
                "fuck".to_string(), "shit".to_string(),
            ]),
            whisper_model: Some("tiny".to_string()),
            description: Some("Mild censoring for minor profanity only".to_string()),
        });
        
        profiles.insert("family".to_string(), ProfileConfig {
            censor_volume: Some(0.05),
            fade_duration: Some(0.2),
            swear_words: Some(vec![
                "fuck".to_string(), "shit".to_string(), "damn".to_string(),
                "hell".to_string(), "ass".to_string(), "bitch".to_string(),
                "bastard".to_string(),
            ]),
            whisper_model: Some("small".to_string()),
            description: Some("Family-friendly censoring profile".to_string()),
        });

        Self {
            whisper_model: Some("base".to_string()),
            censor_volume: Some(0.1),
            fade_duration: Some(0.2),
            swear_words: None, // Use defaults
            output_directory: None,
            show_progress: Some(true),
            language: Some("en".to_string()),
            profiles: Some(profiles),
        }
    }
}

impl ConfigFile {
    /// Load configuration from a YAML file
    pub async fn load_yaml<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref()).await
            .map_err(|e| BabymodeError::FileSystem { 
                source: e, 
                path: path.as_ref().to_path_buf() 
            })?;
        
        serde_yaml::from_str(&contents)
            .map_err(|e| BabymodeError::Config {
                field: "config_file".to_string(),
                message: format!("Failed to parse YAML config: {}", e),
            })
    }

    /// Load configuration from a JSON file
    pub async fn load_json<P: AsRef<Path>>(path: P) -> Result<Self> {
        let contents = fs::read_to_string(path.as_ref()).await
            .map_err(|e| BabymodeError::FileSystem { 
                source: e, 
                path: path.as_ref().to_path_buf() 
            })?;
        
        serde_json::from_str(&contents)
            .map_err(|e| BabymodeError::Config {
                field: "config_file".to_string(),
                message: format!("Failed to parse JSON config: {}", e),
            })
    }

    /// Auto-detect and load configuration file based on extension
    pub async fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        match path.as_ref().extension().and_then(|s| s.to_str()) {
            Some("yaml") | Some("yml") => Self::load_yaml(path).await,
            Some("json") => Self::load_json(path).await,
            Some(ext) => Err(BabymodeError::UnsupportedFormat {
                extension: ext.to_string(),
                supported: vec!["yaml".to_string(), "yml".to_string(), "json".to_string()],
            }),
            None => Err(BabymodeError::Config {
                field: "config_file".to_string(),
                message: "Config file must have .yaml, .yml, or .json extension".to_string(),
            }),
        }
    }

    /// Save configuration to YAML file
    pub async fn save_yaml<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let yaml_content = serde_yaml::to_string(self)
            .map_err(|e| BabymodeError::Config {
                field: "config_file".to_string(),
                message: format!("Failed to serialize config to YAML: {}", e),
            })?;
        
        fs::write(path.as_ref(), yaml_content).await
            .map_err(|e| BabymodeError::FileSystem { 
                source: e, 
                path: path.as_ref().to_path_buf() 
            })
    }

    /// Save configuration to JSON file
    pub async fn save_json<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let json_content = serde_json::to_string_pretty(self)
            .map_err(|e| BabymodeError::Config {
                field: "config_file".to_string(),
                message: format!("Failed to serialize config to JSON: {}", e),
            })?;
        
        fs::write(path.as_ref(), json_content).await
            .map_err(|e| BabymodeError::FileSystem { 
                source: e, 
                path: path.as_ref().to_path_buf() 
            })
    }

    /// Get default config file paths to search
    pub fn default_config_paths() -> Vec<PathBuf> {
        vec![
            PathBuf::from(".babymode.yaml"),
            PathBuf::from(".babymode.yml"),
            PathBuf::from(".babymode.json"),
            dirs::config_dir().unwrap_or_else(|| PathBuf::from("."))
                .join("babymode").join("config.yaml"),
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
                .join(".config").join("babymode.yaml"),
        ]
    }

    /// Try to load configuration from default locations
    pub async fn load_from_default_locations() -> Option<Self> {
        for path in Self::default_config_paths() {
            if path.exists() {
                match Self::load(&path).await {
                    Ok(config) => {
                        log::info!("Loaded configuration from: {}", path.display());
                        return Some(config);
                    }
                    Err(e) => {
                        log::warn!("Failed to load config from {}: {}", path.display(), e);
                    }
                }
            }
        }
        None
    }

    /// Apply this config file to a ConfigBuilder
    pub fn apply_to_builder(&self, mut builder: ConfigBuilder) -> Result<ConfigBuilder> {
        if let Some(ref model_str) = self.whisper_model {
            let model: WhisperModel = model_str.parse()?;
            builder = builder.whisper_model(model);
        }

        if let Some(volume) = self.censor_volume {
            builder = builder.censor_volume(volume)?;
        }

        if let Some(fade) = self.fade_duration {
            builder = builder.fade_duration(fade)?;
        }

        if let Some(ref words) = self.swear_words {
            builder = builder.swear_words(words.clone())?;
        }

        Ok(builder)
    }

    /// Apply a specific profile to a ConfigBuilder
    pub fn apply_profile_to_builder(&self, profile_name: &str, builder: ConfigBuilder) -> Result<ConfigBuilder> {
        let profiles = self.profiles.as_ref().ok_or_else(|| BabymodeError::Config {
            field: "profiles".to_string(),
            message: "No profiles defined".to_string(),
        })?;

        let profile = profiles.get(profile_name).ok_or_else(|| BabymodeError::Config {
            field: "profile".to_string(),
            message: format!("Profile '{}' not found", profile_name),
        })?;

        // First apply base config, then override with profile
        let mut builder = self.apply_to_builder(builder)?;

        if let Some(ref model_str) = profile.whisper_model {
            let model: WhisperModel = model_str.parse()?;
            builder = builder.whisper_model(model);
        }

        if let Some(volume) = profile.censor_volume {
            builder = builder.censor_volume(volume)?;
        }

        if let Some(fade) = profile.fade_duration {
            builder = builder.fade_duration(fade)?;
        }

        if let Some(ref words) = profile.swear_words {
            builder = builder.swear_words(words.clone())?;
        }

        Ok(builder)
    }

    /// List available profiles
    pub fn list_profiles(&self) -> Vec<String> {
        self.profiles
            .as_ref()
            .map(|p| p.keys().cloned().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_config_file_yaml_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test.yaml");
        
        let original_config = ConfigFile::default();
        
        // Save and load
        original_config.save_yaml(&config_path).await.unwrap();
        let loaded_config = ConfigFile::load_yaml(&config_path).await.unwrap();
        
        assert_eq!(original_config.whisper_model, loaded_config.whisper_model);
        assert_eq!(original_config.censor_volume, loaded_config.censor_volume);
    }

    #[tokio::test]
    async fn test_config_file_json_roundtrip() {
        let temp_dir = tempdir().unwrap();
        let config_path = temp_dir.path().join("test.json");
        
        let original_config = ConfigFile::default();
        
        // Save and load
        original_config.save_json(&config_path).await.unwrap();
        let loaded_config = ConfigFile::load_json(&config_path).await.unwrap();
        
        assert_eq!(original_config.whisper_model, loaded_config.whisper_model);
        assert_eq!(original_config.censor_volume, loaded_config.censor_volume);
    }

    #[test]
    fn test_profile_listing() {
        let config = ConfigFile::default();
        let profiles = config.list_profiles();
        
        assert!(profiles.contains(&"strict".to_string()));
        assert!(profiles.contains(&"mild".to_string()));
        assert!(profiles.contains(&"family".to_string()));
    }

    #[tokio::test]
    async fn test_apply_profile() {
        let config = ConfigFile::default();
        let builder = ConfigBuilder::new();
        
        // Apply strict profile
        let result = config.apply_profile_to_builder("strict", builder);
        assert!(result.is_ok());
    }
}