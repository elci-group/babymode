use std::path::PathBuf;
use crate::error::{config_error, BabymodeError, Result};

/// Whisper model variants
#[derive(Debug, Clone, PartialEq)]
pub enum WhisperModel {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl WhisperModel {
    pub fn as_str(&self) -> &'static str {
        match self {
            WhisperModel::Tiny => "tiny",
            WhisperModel::Base => "base", 
            WhisperModel::Small => "small",
            WhisperModel::Medium => "medium",
            WhisperModel::Large => "large",
        }
    }
}

impl std::str::FromStr for WhisperModel {
    type Err = BabymodeError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "tiny" => Ok(WhisperModel::Tiny),
            "base" => Ok(WhisperModel::Base),
            "small" => Ok(WhisperModel::Small),
            "medium" => Ok(WhisperModel::Medium),
            "large" => Ok(WhisperModel::Large),
            _ => Err(config_error(
                "whisper_model",
                format!("Invalid model '{}'. Valid options: tiny, base, small, medium, large", s)
            )),
        }
    }
}

/// Configuration structure for the babymode application
#[derive(Debug, Clone)]
pub struct Config {
    pub input_file: PathBuf,
    pub output_file: Option<PathBuf>,
    pub whisper_model: WhisperModel,
    pub censor_volume: f32,
    pub fade_duration: f32,
    pub swear_words: Vec<String>,
}

impl Config {
    /// Create a new config builder
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Validate input file exists
        if !self.input_file.exists() {
            return Err(config_error(
                "input_file",
                format!("Input file does not exist: {}", self.input_file.display())
            ));
        }

        // Validate input file is a file (not directory)
        if !self.input_file.is_file() {
            return Err(config_error(
                "input_file", 
                format!("Input path is not a file: {}", self.input_file.display())
            ));
        }

        // Validate volume range
        if !(0.0..=1.0).contains(&self.censor_volume) {
            return Err(config_error(
                "censor_volume",
                format!("Volume must be between 0.0 and 1.0, got {}", self.censor_volume)
            ));
        }

        // Validate fade duration
        if self.fade_duration < 0.0 || self.fade_duration > 5.0 {
            return Err(config_error(
                "fade_duration",
                format!("Fade duration must be between 0.0 and 5.0 seconds, got {}", self.fade_duration)
            ));
        }

        // Validate swear words list is not empty
        if self.swear_words.is_empty() {
            return Err(config_error(
                "swear_words",
                "Swear words list cannot be empty"
            ));
        }

        Ok(())
    }

    /// Generate output filename if not provided
    pub fn ensure_output_file(&mut self) -> Result<()> {
        if self.output_file.is_none() {
            let input_stem = self.input_file
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or_else(|| config_error("input_file", "Invalid filename"))?;
                
            let input_ext = self.input_file
                .extension()
                .and_then(|s| s.to_str())
                .unwrap_or("mp4");
            
            let mut output_path = self.input_file.clone();
            output_path.set_file_name(format!("{}_censored.{}", input_stem, input_ext));
            self.output_file = Some(output_path);
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            input_file: PathBuf::new(),
            output_file: None,
            whisper_model: WhisperModel::Base,
            censor_volume: 0.1, // 10% volume during censoring
            fade_duration: 0.2, // 200ms fade in/out
            swear_words: vec![
                "fuck".to_string(),
                "shit".to_string(),
                "damn".to_string(),
                "hell".to_string(),
                "ass".to_string(),
                "bitch".to_string(),
                "bastard".to_string(),
            ],
        }
    }
}

/// Builder pattern for Config
#[derive(Debug, Default)]
pub struct ConfigBuilder {
    input_file: Option<PathBuf>,
    output_file: Option<PathBuf>,
    whisper_model: Option<WhisperModel>,
    censor_volume: Option<f32>,
    fade_duration: Option<f32>,
    swear_words: Option<Vec<String>>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn input_file(mut self, path: PathBuf) -> Self {
        self.input_file = Some(path);
        self
    }

    pub fn output_file(mut self, path: PathBuf) -> Self {
        self.output_file = Some(path);
        self
    }

    pub fn whisper_model(mut self, model: WhisperModel) -> Self {
        self.whisper_model = Some(model);
        self
    }

    pub fn censor_volume(mut self, volume: f32) -> Result<Self> {
        if !(0.0..=1.0).contains(&volume) {
            return Err(config_error(
                "censor_volume",
                format!("Volume must be between 0.0 and 1.0, got {}", volume)
            ));
        }
        self.censor_volume = Some(volume);
        Ok(self)
    }

    pub fn fade_duration(mut self, duration: f32) -> Result<Self> {
        if !(0.0..=5.0).contains(&duration) {
            return Err(config_error(
                "fade_duration",
                format!("Fade duration must be between 0.0 and 5.0 seconds, got {}", duration)
            ));
        }
        self.fade_duration = Some(duration);
        Ok(self)
    }

    pub fn swear_words(mut self, words: Vec<String>) -> Result<Self> {
        if words.is_empty() {
            return Err(config_error("swear_words", "Swear words list cannot be empty"));
        }
        // Normalize to lowercase
        let normalized_words: Vec<String> = words.into_iter()
            .map(|w| w.trim().to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
            
        if normalized_words.is_empty() {
            return Err(config_error("swear_words", "No valid words provided"));
        }
        
        self.swear_words = Some(normalized_words);
        Ok(self)
    }

    pub fn build(self) -> Result<Config> {
        let input_file = self.input_file
            .ok_or_else(|| config_error("input_file", "Input file is required"))?;

        let mut config = Config {
            input_file,
            output_file: self.output_file,
            whisper_model: self.whisper_model.unwrap_or(WhisperModel::Base),
            censor_volume: self.censor_volume.unwrap_or(0.1),
            fade_duration: self.fade_duration.unwrap_or(0.2),
            swear_words: self.swear_words.unwrap_or_else(|| Config::default().swear_words),
        };

        config.validate()?;
        config.ensure_output_file()?;
        
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_whisper_model_parsing() {
        assert_eq!("tiny".parse::<WhisperModel>().unwrap(), WhisperModel::Tiny);
        assert_eq!("BASE".parse::<WhisperModel>().unwrap(), WhisperModel::Base);
        assert!("invalid".parse::<WhisperModel>().is_err());
    }

    #[test]
    fn test_config_builder() {
        let temp_dir = tempdir().unwrap();
        let input_path = temp_dir.path().join("test.mp4");
        File::create(&input_path).unwrap();

        let config = Config::builder()
            .input_file(input_path)
            .censor_volume(0.2).unwrap()
            .fade_duration(0.5).unwrap()
            .build()
            .unwrap();

        assert_eq!(config.censor_volume, 0.2);
        assert_eq!(config.fade_duration, 0.5);
    }

    #[test]
    fn test_config_validation() {
        let config = Config {
            input_file: PathBuf::from("/nonexistent/file.mp4"),
            censor_volume: 1.5, // Invalid
            ..Default::default()
        };
        
        assert!(config.validate().is_err());
    }
}