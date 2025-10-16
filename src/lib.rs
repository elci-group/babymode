// Core modules
pub mod audio;
pub mod censoring;
pub mod config;
pub mod config_file;
pub mod dependencies;
pub mod error;
pub mod plugins;
pub mod progress;
pub mod resources;
pub mod video;
pub mod whisper;

// Re-export commonly used types
pub use audio::{AudioConfig, AudioSegment};
pub use censoring::{CensorConfig, CensorStrategy, CensoringStats};
pub use config::{Config, ConfigBuilder, WhisperModel};
pub use config_file::{ConfigFile, ProfileConfig};
pub use error::{BabymodeError, Result};
pub use plugins::{CensoringStrategy, StrategyRegistry, CensoringConfig};
pub use progress::{ProgressTracker, ProgressOperation};
pub use resources::TempFile;
pub use video::VideoMetadata;
pub use whisper::{WordDetection, merge_detections};
