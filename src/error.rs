use std::fmt;

/// Custom error types for babymode application
#[derive(Debug)]
pub enum BabymodeError {
    /// File system related errors
    FileSystem { source: std::io::Error, path: std::path::PathBuf },
    
    /// FFmpeg related errors
    FFmpeg { message: String, stderr: Option<String> },
    
    /// Python/Whisper related errors
    Whisper { message: String, stderr: Option<String> },
    
    /// Configuration validation errors
    Config { field: String, message: String },
    
    /// Audio processing errors
    AudioProcessing { message: String },
    
    /// Video processing errors
    VideoProcessing { message: String },
    
    /// Unsupported file format
    UnsupportedFormat { extension: String, supported: Vec<String> },
    
    /// Missing external dependency
    MissingDependency { name: String, suggestion: String },
    
    /// General processing error
    Processing { message: String },
}

impl fmt::Display for BabymodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BabymodeError::FileSystem { source, path } => {
                write!(f, "File system error for '{}': {}", path.display(), source)
            }
            BabymodeError::FFmpeg { message, stderr } => {
                write!(f, "FFmpeg error: {}", message)?;
                if let Some(stderr) = stderr {
                    write!(f, "\nStderr: {}", stderr)?;
                }
                Ok(())
            }
            BabymodeError::Whisper { message, stderr } => {
                write!(f, "Whisper error: {}", message)?;
                if let Some(stderr) = stderr {
                    write!(f, "\nStderr: {}", stderr)?;
                }
                Ok(())
            }
            BabymodeError::Config { field, message } => {
                write!(f, "Configuration error in '{}': {}", field, message)
            }
            BabymodeError::AudioProcessing { message } => {
                write!(f, "Audio processing error: {}", message)
            }
            BabymodeError::VideoProcessing { message } => {
                write!(f, "Video processing error: {}", message)
            }
            BabymodeError::UnsupportedFormat { extension, supported } => {
                write!(
                    f,
                    "Unsupported file format '{}'. Supported formats: {}",
                    extension,
                    supported.join(", ")
                )
            }
            BabymodeError::MissingDependency { name, suggestion } => {
                write!(f, "Missing dependency '{}': {}", name, suggestion)
            }
            BabymodeError::Processing { message } => {
                write!(f, "Processing error: {}", message)
            }
        }
    }
}

impl std::error::Error for BabymodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            BabymodeError::FileSystem { source, .. } => Some(source),
            _ => None,
        }
    }
}

/// Result type alias for babymode operations
pub type Result<T> = std::result::Result<T, BabymodeError>;

/// Helper function to create FFmpeg errors
pub fn ffmpeg_error(message: impl Into<String>, stderr: Option<String>) -> BabymodeError {
    BabymodeError::FFmpeg {
        message: message.into(),
        stderr,
    }
}

/// Helper function to create Whisper errors
pub fn whisper_error(message: impl Into<String>, stderr: Option<String>) -> BabymodeError {
    BabymodeError::Whisper {
        message: message.into(),
        stderr,
    }
}

/// Helper function to create configuration errors
pub fn config_error(field: impl Into<String>, message: impl Into<String>) -> BabymodeError {
    BabymodeError::Config {
        field: field.into(),
        message: message.into(),
    }
}

/// Helper function to create file system errors
pub fn fs_error(source: std::io::Error, path: std::path::PathBuf) -> BabymodeError {
    BabymodeError::FileSystem { source, path }
}

/// Trait for converting external errors to BabymodeError
pub trait IntoBabymodeError<T> {
    fn with_path(self, path: std::path::PathBuf) -> Result<T>;
    fn with_context(self, message: impl Into<String>) -> Result<T>;
}

impl<T> IntoBabymodeError<T> for std::result::Result<T, std::io::Error> {
    fn with_path(self, path: std::path::PathBuf) -> Result<T> {
        self.map_err(|e| fs_error(e, path))
    }

    fn with_context(self, message: impl Into<String>) -> Result<T> {
        self.map_err(|e| BabymodeError::Processing {
            message: format!("{}: {}", message.into(), e),
        })
    }
}

// Conversion from anyhow::Error to BabymodeError for compatibility
impl From<anyhow::Error> for BabymodeError {
    fn from(err: anyhow::Error) -> Self {
        BabymodeError::Processing {
            message: err.to_string(),
        }
    }
}
