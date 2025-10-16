use crate::audio::AudioSegment;
use crate::error::{BabymodeError, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::process::Command;
use log::{debug, info};

/// Trait defining a censoring strategy plugin
#[async_trait]
pub trait CensoringStrategy: Send + Sync {
    /// Name of the strategy
    fn name(&self) -> &str;
    
    /// Description of what this strategy does
    fn description(&self) -> &str;
    
    /// Apply censoring to the given audio segments
    async fn apply_censoring(
        &self,
        input_path: &Path,
        output_path: &Path,
        segments: &[AudioSegment],
        config: &CensoringConfig,
    ) -> Result<()>;
    
    /// Validate configuration for this strategy
    fn validate_config(&self, config: &CensoringConfig) -> Result<()> {
        // Default implementation - no validation required
        let _ = config;
        Ok(())
    }
}

/// Configuration for censoring strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CensoringConfig {
    pub volume: f32,
    pub fade_duration: f32,
    pub replacement_audio: Option<String>,
    pub beep_frequency: Option<f32>,
    pub custom_params: HashMap<String, serde_json::Value>,
}

impl Default for CensoringConfig {
    fn default() -> Self {
        Self {
            volume: 0.1,
            fade_duration: 0.2,
            replacement_audio: None,
            beep_frequency: Some(1000.0),
            custom_params: HashMap::new(),
        }
    }
}

/// Registry of available censoring strategies
pub struct StrategyRegistry {
    strategies: HashMap<String, Box<dyn CensoringStrategy>>,
}

impl StrategyRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            strategies: HashMap::new(),
        };
        
        // Register built-in strategies
        registry.register(Box::new(SilenceStrategy));
        registry.register(Box::new(VolumeReductionStrategy));
        registry.register(Box::new(BeepStrategy));
        registry.register(Box::new(ReverseAudioStrategy));
        
        registry
    }
    
    pub fn register(&mut self, strategy: Box<dyn CensoringStrategy>) {
        let name = strategy.name().to_string();
        self.strategies.insert(name, strategy);
    }
    
    pub fn get_strategy(&self, name: &str) -> Option<&dyn CensoringStrategy> {
        self.strategies.get(name).map(|s| s.as_ref())
    }
    
    pub fn list_strategies(&self) -> Vec<(&str, &str)> {
        self.strategies
            .values()
            .map(|s| (s.name(), s.description()))
            .collect()
    }
    
    pub async fn apply_strategy(
        &self,
        strategy_name: &str,
        input_path: &Path,
        output_path: &Path,
        segments: &[AudioSegment],
        config: &CensoringConfig,
    ) -> Result<()> {
        let strategy = self.get_strategy(strategy_name)
            .ok_or_else(|| BabymodeError::Processing {
                message: format!("Unknown censoring strategy: {}", strategy_name),
            })?;
        
        strategy.validate_config(config)?;
        strategy.apply_censoring(input_path, output_path, segments, config).await
    }
}

impl Default for StrategyRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Complete silence strategy - replaces profanity with silence
pub struct SilenceStrategy;

#[async_trait]
impl CensoringStrategy for SilenceStrategy {
    fn name(&self) -> &str {
        "silence"
    }
    
    fn description(&self) -> &str {
        "Replace profanity with complete silence"
    }
    
    async fn apply_censoring(
        &self,
        input_path: &Path,
        output_path: &Path,
        segments: &[AudioSegment],
        _config: &CensoringConfig,
    ) -> Result<()> {
        if segments.is_empty() {
            tokio::fs::copy(input_path, output_path).await
                .map_err(|e| BabymodeError::Processing { 
                    message: format!("Failed to copy audio: {}", e) 
                })?;
            return Ok(());
        }

        let mut volume_conditions = Vec::new();
        
        for segment in segments {
            volume_conditions.push(format!(
                "volume=enable='between(t,{:.3},{:.3})':volume=0",
                segment.start_time, segment.end_time
            ));
        }
        
        let filter_complex = volume_conditions.join(",");
        
        let output = Command::new("ffmpeg")
            .args([
                "-i", input_path.to_str().unwrap(),
                "-af", &filter_complex,
                "-c:a", "pcm_s16le",
                "-y",
                output_path.to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| BabymodeError::Processing { 
                message: format!("FFmpeg failed: {}", e) 
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(BabymodeError::Processing {
                message: format!("FFmpeg failed with silence strategy: {}", error),
            });
        }

        info!("Applied silence censoring to {} segments", segments.len());
        Ok(())
    }
}

/// Volume reduction strategy - reduces volume during profanity
pub struct VolumeReductionStrategy;

#[async_trait]
impl CensoringStrategy for VolumeReductionStrategy {
    fn name(&self) -> &str {
        "volume_reduction"
    }
    
    fn description(&self) -> &str {
        "Reduce volume during profanity with smooth fading"
    }
    
    async fn apply_censoring(
        &self,
        input_path: &Path,
        output_path: &Path,
        segments: &[AudioSegment],
        config: &CensoringConfig,
    ) -> Result<()> {
        if segments.is_empty() {
            tokio::fs::copy(input_path, output_path).await
                .map_err(|e| BabymodeError::Processing { 
                    message: format!("Failed to copy audio: {}", e) 
                })?;
            return Ok(());
        }

        let mut volume_conditions = Vec::new();
        
        for segment in segments {
            // Create fade in and fade out with reduced volume
            let fade_in_end = segment.start_time + config.fade_duration as f64;
            let fade_out_start = segment.end_time - config.fade_duration as f64;
            
            volume_conditions.push(format!(
                "volume=enable='between(t,{:.3},{:.3})':volume='if(lt(t,{:.3}),(t-{:.3})/{:.3}*{:.3},{:.3})'",
                segment.start_time, segment.end_time,
                fade_in_end, segment.start_time, config.fade_duration, config.volume, config.volume
            ));
            
            if fade_out_start > fade_in_end {
                volume_conditions.push(format!(
                    "volume=enable='between(t,{:.3},{:.3})':volume='if(gt(t,{:.3}),({:.3}-t)/{:.3}*{:.3}+1-{:.3},{:.3})'",
                    fade_out_start, segment.end_time,
                    fade_out_start, segment.end_time, config.fade_duration, config.volume, config.volume, config.volume
                ));
            }
        }
        
        let filter_complex = volume_conditions.join(",");
        
        let output = Command::new("ffmpeg")
            .args([
                "-i", input_path.to_str().unwrap(),
                "-af", &filter_complex,
                "-c:a", "pcm_s16le",
                "-y",
                output_path.to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| BabymodeError::Processing { 
                message: format!("FFmpeg failed: {}", e) 
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(BabymodeError::Processing {
                message: format!("FFmpeg failed with volume reduction: {}", error),
            });
        }

        info!("Applied volume reduction to {} segments", segments.len());
        Ok(())
    }
}

/// Beep strategy - replaces profanity with beep sounds
pub struct BeepStrategy;

#[async_trait]
impl CensoringStrategy for BeepStrategy {
    fn name(&self) -> &str {
        "beep"
    }
    
    fn description(&self) -> &str {
        "Replace profanity with beep sounds"
    }
    
    fn validate_config(&self, config: &CensoringConfig) -> Result<()> {
        if let Some(freq) = config.beep_frequency {
            if !(100.0..=10000.0).contains(&freq) {
                return Err(BabymodeError::Config {
                    field: "beep_frequency".to_string(),
                    message: "Beep frequency must be between 100 and 10000 Hz".to_string(),
                });
            }
        }
        Ok(())
    }
    
    async fn apply_censoring(
        &self,
        input_path: &Path,
        output_path: &Path,
        segments: &[AudioSegment],
        config: &CensoringConfig,
    ) -> Result<()> {
        if segments.is_empty() {
            tokio::fs::copy(input_path, output_path).await
                .map_err(|e| BabymodeError::Processing { 
                    message: format!("Failed to copy audio: {}", e) 
                })?;
            return Ok(());
        }

        let frequency = config.beep_frequency.unwrap_or(1000.0);
        let mut filter_parts = vec!["[0:a]".to_string()];
        
        for (i, segment) in segments.iter().enumerate() {
            let duration = segment.end_time - segment.start_time;
            let beep_filter = format!(
                "sine=frequency={}:duration={}[beep{}];",
                frequency, duration, i
            );
            filter_parts.push(beep_filter);
            
            let overlay_filter = format!(
                "[{}][beep{}]amix=inputs=2:duration=first:dropout_transition=0,volume=enable='between(t,{:.3},{:.3})':volume=0[out{}];",
                filter_parts.last().unwrap().trim_end_matches(';'),
                i, segment.start_time, segment.end_time, i
            );
            filter_parts.push(overlay_filter);
        }
        
        // Remove the last semicolon and build final filter
        let mut filter_complex = filter_parts.join("");
        if let Some(last_index) = filter_complex.rfind(';') {
            filter_complex.truncate(last_index);
        }
        
        debug!("Beep filter: {}", filter_complex);
        
        let output = Command::new("ffmpeg")
            .args([
                "-i", input_path.to_str().unwrap(),
                "-filter_complex", &filter_complex,
                "-c:a", "pcm_s16le",
                "-y",
                output_path.to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| BabymodeError::Processing { 
                message: format!("FFmpeg failed: {}", e) 
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(BabymodeError::Processing {
                message: format!("FFmpeg failed with beep strategy: {}", error),
            });
        }

        info!("Applied beep censoring to {} segments", segments.len());
        Ok(())
    }
}

/// Reverse audio strategy - plays profanity backwards
pub struct ReverseAudioStrategy;

#[async_trait]
impl CensoringStrategy for ReverseAudioStrategy {
    fn name(&self) -> &str {
        "reverse"
    }
    
    fn description(&self) -> &str {
        "Play profanity segments in reverse"
    }
    
    async fn apply_censoring(
        &self,
        input_path: &Path,
        output_path: &Path,
        segments: &[AudioSegment],
        _config: &CensoringConfig,
    ) -> Result<()> {
        if segments.is_empty() {
            tokio::fs::copy(input_path, output_path).await
                .map_err(|e| BabymodeError::Processing { 
                    message: format!("Failed to copy audio: {}", e) 
                })?;
            return Ok(());
        }

        // This is a simplified implementation
        // A full implementation would need to extract segments, reverse them, and recombine
        let mut volume_conditions = Vec::new();
        
        for segment in segments {
            volume_conditions.push(format!(
                "areverse=enable='between(t,{:.3},{:.3})'",
                segment.start_time, segment.end_time
            ));
        }
        
        let filter_complex = volume_conditions.join(",");
        
        let output = Command::new("ffmpeg")
            .args([
                "-i", input_path.to_str().unwrap(),
                "-af", &filter_complex,
                "-c:a", "pcm_s16le",
                "-y",
                output_path.to_str().unwrap(),
            ])
            .output()
            .await
            .map_err(|e| BabymodeError::Processing { 
                message: format!("FFmpeg failed: {}", e) 
            })?;

        if !output.status.success() {
            let error = String::from_utf8_lossy(&output.stderr);
            return Err(BabymodeError::Processing {
                message: format!("FFmpeg failed with reverse strategy: {}", error),
            });
        }

        info!("Applied reverse audio censoring to {} segments", segments.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strategy_registry() {
        let registry = StrategyRegistry::new();
        
        assert!(registry.get_strategy("silence").is_some());
        assert!(registry.get_strategy("volume_reduction").is_some());
        assert!(registry.get_strategy("beep").is_some());
        assert!(registry.get_strategy("reverse").is_some());
        assert!(registry.get_strategy("nonexistent").is_none());
        
        let strategies = registry.list_strategies();
        assert!(!strategies.is_empty());
        
        // Check that we have expected strategies
        let strategy_names: Vec<&str> = strategies.iter().map(|(name, _)| *name).collect();
        assert!(strategy_names.contains(&"silence"));
        assert!(strategy_names.contains(&"beep"));
    }

    #[test]
    fn test_censoring_config_validation() {
        let beep_strategy = BeepStrategy;
        
        // Valid config
        let valid_config = CensoringConfig {
            beep_frequency: Some(1000.0),
            ..Default::default()
        };
        assert!(beep_strategy.validate_config(&valid_config).is_ok());
        
        // Invalid frequency
        let invalid_config = CensoringConfig {
            beep_frequency: Some(50000.0), // Too high
            ..Default::default()
        };
        assert!(beep_strategy.validate_config(&invalid_config).is_err());
    }

    #[tokio::test]
    async fn test_strategy_registry_apply() {
        let registry = StrategyRegistry::new();
        let config = CensoringConfig::default();
        let segments = vec![AudioSegment::new(1.0, 2.0)];
        
        // This would normally require actual audio files
        // For now, just test that the method exists and doesn't panic
        let result = registry.apply_strategy(
            "nonexistent",
            Path::new("dummy.wav"),
            Path::new("output.wav"),
            &segments,
            &config,
        ).await;
        
        // Should fail with unknown strategy error
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Unknown censoring strategy"));
    }
}