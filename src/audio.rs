use anyhow::{Context, Result};
use log::{debug, info};
use std::path::Path;
use tokio::process::Command;
use crate::resources::TempFile;

/// Audio format configuration for whisper processing
#[derive(Debug, Clone)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub channels: u8,
    pub format: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 16000, // Whisper prefers 16kHz
            channels: 1,        // Mono audio
            format: "wav".to_string(), // WAV format for whisper
        }
    }
}

/// Audio segment with timing information
#[derive(Debug, Clone)]
pub struct AudioSegment {
    pub start_time: f64,
    pub end_time: f64,
    pub duration: f64,
}

impl AudioSegment {
    pub fn new(start_time: f64, end_time: f64) -> Self {
        Self {
            start_time,
            end_time,
            duration: end_time - start_time,
        }
    }
}

/// Extract audio from video file using ffmpeg
pub async fn extract_audio(video_path: &Path) -> Result<TempFile> {
    // Create temporary directory and file manually
    let temp_dir = std::env::temp_dir();
    let audio_filename = format!("babymode_audio_{}.wav", 
                               std::process::id());
    let audio_path = temp_dir.join(audio_filename);
    
    info!("Extracting audio from {:?} to {:?}", video_path, audio_path);

    let config = AudioConfig::default();

    let output = Command::new("ffmpeg")
        .args([
            "-i", video_path.to_str().context("Invalid video path")?,
            "-vn", // No video
            "-acodec", "pcm_s16le", // 16-bit PCM
            "-ar", &config.sample_rate.to_string(), // Sample rate
            "-ac", &config.channels.to_string(), // Mono
            "-y", // Overwrite output file
            audio_path.to_str().context("Invalid audio path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to extract audio: {}", error);
    }

    let temp_file = TempFile::new(audio_path);

    // Verify the audio file was created
    if !temp_file.exists() {
        anyhow::bail!("Audio extraction failed - output file not created");
    }

    debug!("Audio extracted successfully to: {:?}", temp_file.path());
    
    Ok(temp_file)
}

/// Get audio duration using ffprobe
pub async fn get_audio_duration(audio_path: &Path) -> Result<f64> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            audio_path.to_str().context("Invalid audio path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffprobe")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffprobe failed: {}", error);
    }

    let json_output = String::from_utf8(output.stdout)
        .context("ffprobe output is not valid UTF-8")?;

    let probe_data: serde_json::Value = serde_json::from_str(&json_output)
        .context("Failed to parse ffprobe JSON output")?;

    let duration: f64 = probe_data
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse().ok())
        .context("Could not parse audio duration")?;

    debug!("Audio duration: {:.2} seconds", duration);
    Ok(duration)
}

/// Apply volume changes to audio segments
pub async fn apply_volume_changes(
    input_path: &Path,
    output_path: &Path,
    volume_segments: &[(AudioSegment, f32)], // (segment, volume_factor)
) -> Result<()> {
    info!("Applying volume changes to audio file");

    if volume_segments.is_empty() {
        // No changes needed, just copy the file
        tokio::fs::copy(input_path, output_path).await
            .context("Failed to copy audio file")?;
        return Ok(());
    }

    // Build ffmpeg filter for volume changes
    let mut filter_complex = String::new();
    let mut current_input = "[0:a]".to_string();

    for (i, (segment, volume)) in volume_segments.iter().enumerate() {
        let start_time = segment.start_time;
        let end_time = segment.end_time;
        
        // Create volume filter for this segment
        let filter_name = format!("vol{}", i);
        let volume_filter = format!(
            "{}volume=enable='between(t,{},{})':{volume}[{}];",
            current_input, start_time, end_time, filter_name
        );
        
        filter_complex.push_str(&volume_filter);
        current_input = format!("[{}]", filter_name);
    }

    // Remove the trailing semicolon and add final output
    filter_complex = filter_complex.trim_end_matches(';').to_string();

    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().context("Invalid input path")?,
            "-filter_complex", &filter_complex,
            "-c:a", "pcm_s16le", // Keep same codec
            "-y", // Overwrite output
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg for volume changes")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to apply volume changes: {}", error);
    }

    info!("Successfully applied volume changes to: {:?}", output_path);
    Ok(())
}

/// Apply isolation and inversion censoring to completely remove profanity
pub async fn apply_isolation_censoring(
    input_path: &Path,
    output_path: &Path,
    censor_segments: &[AudioSegment],
    _fade_duration: f32,
) -> Result<()> {
    info!("Applying isolation censoring to {} segments", censor_segments.len());

    if censor_segments.is_empty() {
        tokio::fs::copy(input_path, output_path).await
            .context("Failed to copy audio file")?;
        return Ok(());
    }

    // Build volume filter that sets volume to 0 for each segment
    let mut volume_conditions = Vec::new();
    
    for segment in censor_segments.iter() {
        // Apply complete silence (volume=0) for this segment
        volume_conditions.push(format!(
            "volume=enable='between(t,{:.3},{:.3})':volume=0",
            segment.start_time, segment.end_time
        ));
    }
    
    let filter_complex = volume_conditions.join(",");
    debug!("Silence filter: {}", filter_complex);

    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().context("Invalid input path")?,
            "-af", &filter_complex,
            "-c:a", "pcm_s16le",
            "-y",
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg for isolation censoring")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to apply isolation censoring: {}", error);
    }

    info!("Successfully applied isolation censoring to: {:?}", output_path);
    Ok(())
}

/// Apply smooth fade in/out to audio segments for natural censoring (legacy)
pub async fn apply_smooth_censoring(
    input_path: &Path,
    output_path: &Path,
    censor_segments: &[AudioSegment],
    _target_volume: f32,
    fade_duration: f32,
) -> Result<()> {
    // Use isolation censoring for more effective results
    apply_isolation_censoring(input_path, output_path, censor_segments, fade_duration).await
}

/// Convert audio to format suitable for Whisper
pub async fn convert_for_whisper(input_path: &Path, output_path: &Path) -> Result<()> {
    let config = AudioConfig::default();
    
    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().context("Invalid input path")?,
            "-ar", &config.sample_rate.to_string(),
            "-ac", &config.channels.to_string(),
            "-c:a", "pcm_s16le",
            "-y",
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg for whisper conversion")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to convert audio for whisper: {}", error);
    }

    debug!("Audio converted for whisper: {:?}", output_path);
    Ok(())
}

/// Extract audio segment from a specific time range
pub async fn extract_audio_segment(
    input_path: &Path,
    output_path: &Path,
    start_time: f64,
    duration: f64,
) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().context("Invalid input path")?,
            "-ss", &start_time.to_string(), // Start time
            "-t", &duration.to_string(),    // Duration
            "-c:a", "copy", // Copy audio codec
            "-y",
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg for segment extraction")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to extract audio segment: {}", error);
    }

    debug!("Audio segment extracted: {:.2}s-{:.2}s to {:?}", 
           start_time, start_time + duration, output_path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_segment_creation() {
        let segment = AudioSegment::new(10.5, 15.2);
        assert_eq!(segment.start_time, 10.5);
        assert_eq!(segment.end_time, 15.2);
        assert!((segment.duration - 4.7).abs() < 1e-10);
    }

    #[test]
    fn test_audio_config_default() {
        let config = AudioConfig::default();
        assert_eq!(config.sample_rate, 16000);
        assert_eq!(config.channels, 1);
        assert_eq!(config.format, "wav");
    }
}