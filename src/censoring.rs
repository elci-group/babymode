use anyhow::{Context, Result};
use log::{debug, info};
use std::path::Path;

use crate::audio::{AudioSegment, apply_smooth_censoring};
use crate::resources::TempFile;
use crate::whisper::{WordDetection, merge_detections};
use crate::Config;

/// Censoring strategy options
#[derive(Debug, Clone)]
pub enum CensorStrategy {
    /// Reduce volume to a specified level
    VolumeReduction(f32),
    /// Replace with silence
    Silence,
    /// Replace with beep sound
    Beep(f32), // frequency in Hz
    /// Replace with white noise
    WhiteNoise(f32), // volume level
}

impl Default for CensorStrategy {
    fn default() -> Self {
        CensorStrategy::VolumeReduction(0.1) // 10% volume
    }
}

/// Censoring configuration
#[derive(Debug, Clone)]
pub struct CensorConfig {
    pub strategy: CensorStrategy,
    pub fade_duration: f32,
    pub merge_gap: f32, // Gap between detections to merge (in seconds)
    pub padding: f32,   // Extra padding around detected words (in seconds)
}

impl From<&Config> for CensorConfig {
    fn from(config: &Config) -> Self {
        Self {
            strategy: CensorStrategy::VolumeReduction(config.censor_volume),
            fade_duration: config.fade_duration,
            merge_gap: 0.5, // Merge detections within 0.5 seconds
            padding: 0.1,   // 100ms padding around each word
        }
    }
}

/// Apply censoring to audio based on word detections
pub async fn apply_censoring(
    input_audio_path: &Path,
    detections: &[WordDetection],
    config: &Config,
) -> Result<TempFile> {
    // Create manual temp file path that persists
    let temp_dir = std::env::temp_dir();
    let audio_filename = format!("babymode_censored_{}.wav", std::process::id());
    let output_path = temp_dir.join(audio_filename);
    
    info!("Applying censoring to {} detected words", detections.len());
    
    let censor_config = CensorConfig::from(config);
    
    // Merge nearby detections to avoid choppy audio
    let audio_segments = merge_detections(detections.to_vec(), censor_config.merge_gap as f64);
    
    // Add padding to segments
    let padded_segments = add_padding_to_segments(audio_segments, censor_config.padding);
    
    // Apply the censoring strategy
    match censor_config.strategy {
        CensorStrategy::VolumeReduction(volume) => {
            apply_volume_censoring(
                input_audio_path,
                &output_path,
                &padded_segments,
                volume,
                censor_config.fade_duration,
            ).await?;
        }
        CensorStrategy::Silence => {
            apply_silence_censoring(
                input_audio_path,
                &output_path,
                &padded_segments,
                censor_config.fade_duration,
            ).await?;
        }
        CensorStrategy::Beep(frequency) => {
            apply_beep_censoring(
                input_audio_path,
                &output_path,
                &padded_segments,
                frequency,
                censor_config.fade_duration,
            ).await?;
        }
        CensorStrategy::WhiteNoise(volume) => {
            apply_noise_censoring(
                input_audio_path,
                &output_path,
                &padded_segments,
                volume,
                censor_config.fade_duration,
            ).await?;
        }
    }
    
    let temp_file = TempFile::new(output_path);
    info!("Censoring applied successfully to: {:?}", temp_file.path());
    Ok(temp_file)
}

/// Add padding around segments to ensure smooth transitions
fn add_padding_to_segments(segments: Vec<AudioSegment>, padding: f32) -> Vec<AudioSegment> {
    segments.into_iter()
        .map(|segment| {
            let new_start = (segment.start_time - padding as f64).max(0.0);
            let new_end = segment.end_time + padding as f64;
            AudioSegment::new(new_start, new_end)
        })
        .collect()
}

/// Apply volume reduction censoring with smooth fades
async fn apply_volume_censoring(
    input_path: &Path,
    output_path: &Path,
    segments: &[AudioSegment],
    target_volume: f32,
    fade_duration: f32,
) -> Result<()> {
    debug!("Applying volume reduction censoring (volume: {:.2}, fade: {:.2}s)", 
           target_volume, fade_duration);
    
    apply_smooth_censoring(
        input_path,
        output_path,
        segments,
        target_volume,
        fade_duration,
    ).await
}

/// Apply silence censoring with smooth fades
async fn apply_silence_censoring(
    input_path: &Path,
    output_path: &Path,
    segments: &[AudioSegment],
    fade_duration: f32,
) -> Result<()> {
    debug!("Applying silence censoring (fade: {:.2}s)", fade_duration);
    
    // Silence is just volume reduction to 0
    apply_smooth_censoring(
        input_path,
        output_path,
        segments,
        0.0,
        fade_duration,
    ).await
}

/// Apply beep censoring - replace swear words with a beep tone
async fn apply_beep_censoring(
    input_path: &Path,
    output_path: &Path,
    segments: &[AudioSegment],
    frequency: f32,
    fade_duration: f32,
) -> Result<()> {
    use tokio::process::Command;
    
    debug!("Applying beep censoring (freq: {:.0}Hz, fade: {:.2}s)", frequency, fade_duration);
    
    if segments.is_empty() {
        tokio::fs::copy(input_path, output_path).await
            .context("Failed to copy audio file")?;
        return Ok(());
    }
    
    // Build complex filter for beep replacement
    let mut filters = Vec::new();
    
    // Start with the original audio
    filters.push("[0:a]".to_string());
    
    for (i, segment) in segments.iter().enumerate() {
        let beep_duration = segment.duration;
        
        // Generate a sine wave beep for this segment
        let beep_filter = format!(
            "sine=frequency={}:duration={}:sample_rate=16000[beep{}]",
            frequency, beep_duration, i
        );
        filters.push(beep_filter);
        
        // Replace the audio segment with the beep
        let replace_filter = format!(
            "[0:a][beep{}]amix=inputs=2:duration=first:dropout_transition={}[mixed{}]",
            i, fade_duration, i
        );
        filters.push(replace_filter);
    }
    
    let filter_complex = filters.join(";");
    
    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().context("Invalid input path")?,
            "-filter_complex", &filter_complex,
            "-c:a", "pcm_s16le",
            "-y",
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg for beep censoring")?;
    
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to apply beep censoring: {}", error);
    }
    
    Ok(())
}

/// Apply white noise censoring - replace swear words with white noise
async fn apply_noise_censoring(
    input_path: &Path,
    output_path: &Path,
    segments: &[AudioSegment],
    noise_volume: f32,
    fade_duration: f32,
) -> Result<()> {
    use tokio::process::Command;
    
    debug!("Applying white noise censoring (volume: {:.2}, fade: {:.2}s)", 
           noise_volume, fade_duration);
    
    if segments.is_empty() {
        tokio::fs::copy(input_path, output_path).await
            .context("Failed to copy audio file")?;
        return Ok(());
    }
    
    // Build filter to replace segments with white noise
    let mut filters = Vec::new();
    
    for (i, segment) in segments.iter().enumerate() {
        // Create white noise for the duration of this segment
        let noise_duration = segment.duration;
        let noise_filter = format!(
            "anoisesrc=duration={}:sample_rate=16000:amplitude={}[noise{}]",
            noise_duration, noise_volume, i
        );
        filters.push(noise_filter);
        
        // Apply the noise with smooth transitions
        let enable_condition = format!("between(t,{},{})", segment.start_time, segment.end_time);
        let mix_filter = format!(
            "[0:a][noise{}]amix=inputs=2:duration=first:dropout_transition={}:enable='{}'[mixed{}]",
            i, fade_duration, enable_condition, i
        );
        filters.push(mix_filter);
    }
    
    let filter_complex = filters.join(";");
    
    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().context("Invalid input path")?,
            "-filter_complex", &filter_complex,
            "-c:a", "pcm_s16le",
            "-y",
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg for noise censoring")?;
    
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to apply noise censoring: {}", error);
    }
    
    Ok(())
}

/// Preview censoring effects without writing to file
pub async fn preview_censoring(
    _input_audio_path: &Path,
    detections: &[WordDetection],
    config: &Config,
) -> Result<Vec<AudioSegment>> {
    let censor_config = CensorConfig::from(config);
    
    // Merge nearby detections
    let audio_segments = merge_detections(detections.to_vec(), censor_config.merge_gap as f64);
    
    // Add padding to segments
    let padded_segments = add_padding_to_segments(audio_segments, censor_config.padding);
    
    info!("Preview: {} segments will be censored", padded_segments.len());
    for (i, segment) in padded_segments.iter().enumerate() {
        info!("Segment {}: {:.2}s - {:.2}s ({:.2}s duration)", 
              i + 1, segment.start_time, segment.end_time, segment.duration);
    }
    
    Ok(padded_segments)
}

/// Get statistics about censoring operations
#[derive(Debug)]
pub struct CensoringStats {
    pub total_detections: usize,
    pub merged_segments: usize,
    pub total_censored_duration: f64,
    pub percentage_censored: f64,
    pub audio_duration: f64,
}

pub async fn get_censoring_stats(
    audio_path: &Path,
    detections: &[WordDetection],
    config: &Config,
) -> Result<CensoringStats> {
    let censor_config = CensorConfig::from(config);
    let audio_duration = crate::audio::get_audio_duration(audio_path).await?;
    
    // Merge nearby detections
    let audio_segments = merge_detections(detections.to_vec(), censor_config.merge_gap as f64);
    let padded_segments = add_padding_to_segments(audio_segments, censor_config.padding);
    
    let total_censored_duration: f64 = padded_segments.iter()
        .map(|s| s.duration)
        .sum();
    
    let percentage_censored = if audio_duration > 0.0 {
        (total_censored_duration / audio_duration) * 100.0
    } else {
        0.0
    };
    
    Ok(CensoringStats {
        total_detections: detections.len(),
        merged_segments: padded_segments.len(),
        total_censored_duration,
        percentage_censored,
        audio_duration,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_padding_to_segments() {
        let segments = vec![
            AudioSegment::new(10.0, 11.0),
            AudioSegment::new(15.0, 16.5),
        ];
        
        let padded = add_padding_to_segments(segments, 0.2);
        
        assert_eq!(padded.len(), 2);
        assert!((padded[0].start_time - 9.8).abs() < 1e-6);
        assert!((padded[0].end_time - 11.2).abs() < 1e-6);
        assert!((padded[1].start_time - 14.8).abs() < 1e-6);
        assert!((padded[1].end_time - 16.7).abs() < 1e-6);
    }

    #[test]
    fn test_padding_prevents_negative_time() {
        let segments = vec![AudioSegment::new(0.1, 0.5)];
        let padded = add_padding_to_segments(segments, 0.2);
        
        assert_eq!(padded[0].start_time, 0.0); // Should not go below 0
        assert!((padded[0].end_time - 0.7).abs() < 1e-6);
    }

    #[test]
    fn test_censor_config_from_config() {
        let config = Config {
            censor_volume: 0.05,
            fade_duration: 0.3,
            ..Default::default()
        };
        
        let censor_config = CensorConfig::from(&config);
        
        match censor_config.strategy {
            CensorStrategy::VolumeReduction(volume) => {
                assert_eq!(volume, 0.05);
            }
            _ => panic!("Expected VolumeReduction strategy"),
        }
        
        assert_eq!(censor_config.fade_duration, 0.3);
    }
}