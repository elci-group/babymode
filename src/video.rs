use anyhow::{Context, Result};
use log::{debug, info};
use std::path::Path;
use tokio::process::Command;

/// Supported video file extensions
const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp4", "avi", "mov", "mkv", "wmv", "flv", "webm", "m4v", "3gp", "mpg", "mpeg"
];

/// Video metadata structure
#[derive(Debug)]
pub struct VideoMetadata {
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    pub codec: String,
    pub bitrate: Option<u64>,
}

/// Validate that the given file is a supported video file
pub fn validate_video_file(path: &Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("Video file does not exist: {:?}", path);
    }

    if !path.is_file() {
        anyhow::bail!("Path is not a file: {:?}", path);
    }

    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .context("File has no extension")?
        .to_lowercase();

    if !SUPPORTED_EXTENSIONS.contains(&extension.as_str()) {
        anyhow::bail!(
            "Unsupported video format: {}. Supported formats: {:?}",
            extension,
            SUPPORTED_EXTENSIONS
        );
    }

    debug!("Video file validation passed for: {:?}", path);
    Ok(())
}

/// Get video metadata using ffprobe
pub async fn get_video_metadata(path: &Path) -> Result<VideoMetadata> {
    let output = Command::new("ffprobe")
        .args([
            "-v", "quiet",
            "-print_format", "json",
            "-show_format",
            "-show_streams",
            path.to_str().context("Invalid path encoding")?
        ])
        .output()
        .await
        .context("Failed to execute ffprobe. Make sure ffmpeg is installed.")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffprobe failed: {}", error);
    }

    let json_output = String::from_utf8(output.stdout)
        .context("ffprobe output is not valid UTF-8")?;

    let probe_data: serde_json::Value = serde_json::from_str(&json_output)
        .context("Failed to parse ffprobe JSON output")?;

    // Extract format information
    let format = probe_data.get("format")
        .context("No format information in ffprobe output")?;

    let duration: f64 = format.get("duration")
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse().ok())
        .context("Could not parse video duration")?;

    let bitrate: Option<u64> = format.get("bit_rate")
        .and_then(|d| d.as_str())
        .and_then(|s| s.parse().ok());

    // Extract video stream information
    let streams = probe_data.get("streams")
        .and_then(|s| s.as_array())
        .context("No streams information in ffprobe output")?;

    let mut video_stream = None;
    let mut has_audio = false;

    for stream in streams {
        let codec_type = stream.get("codec_type")
            .and_then(|t| t.as_str())
            .unwrap_or("");

        match codec_type {
            "video" if video_stream.is_none() => {
                video_stream = Some(stream);
            }
            "audio" => {
                has_audio = true;
            }
            _ => {}
        }
    }

    let video_stream = video_stream
        .context("No video stream found in the file")?;

    let width: u32 = video_stream.get("width")
        .and_then(|w| w.as_u64())
        .context("Could not parse video width")? as u32;

    let height: u32 = video_stream.get("height")
        .and_then(|h| h.as_u64())
        .context("Could not parse video height")? as u32;

    let codec = video_stream.get("codec_name")
        .and_then(|c| c.as_str())
        .context("Could not get video codec")?
        .to_string();

    // Parse frame rate
    let fps_str = video_stream.get("r_frame_rate")
        .and_then(|fps| fps.as_str())
        .context("Could not get frame rate")?;

    let fps = parse_frame_rate(fps_str)?;

    let metadata = VideoMetadata {
        duration,
        width,
        height,
        fps,
        has_audio,
        codec,
        bitrate,
    };

    debug!("Video metadata: {:?}", metadata);
    Ok(metadata)
}

/// Parse frame rate string (e.g., "30/1", "25/1") to float
fn parse_frame_rate(fps_str: &str) -> Result<f64> {
    let parts: Vec<&str> = fps_str.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid frame rate format: {}", fps_str);
    }

    let numerator: f64 = parts[0].parse()
        .context("Could not parse frame rate numerator")?;
    let denominator: f64 = parts[1].parse()
        .context("Could not parse frame rate denominator")?;

    if denominator == 0.0 {
        anyhow::bail!("Frame rate denominator is zero");
    }

    Ok(numerator / denominator)
}

/// Combine video with new audio track using ffmpeg
pub async fn combine_video_audio(
    video_path: &Path,
    audio_path: &Path,
    output_path: &Path,
) -> Result<()> {
    info!("Combining video {:?} with audio {:?}", video_path, audio_path);

    let output = Command::new("ffmpeg")
        .args([
            "-i", video_path.to_str().context("Invalid video path")?,
            "-i", audio_path.to_str().context("Invalid audio path")?,
            "-c:v", "copy", // Copy video stream without re-encoding
            "-c:a", "aac",  // Re-encode audio as AAC
            "-map", "0:v:0", // Map first video stream from first input
            "-map", "1:a:0", // Map first audio stream from second input
            "-shortest", // End when shortest stream ends
            "-y", // Overwrite output file if it exists
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to combine video and audio: {}", error);
    }

    info!("Successfully combined video and audio to: {:?}", output_path);
    Ok(())
}

/// Extract video without audio (for testing purposes)
pub async fn extract_video_only(input_path: &Path, output_path: &Path) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args([
            "-i", input_path.to_str().context("Invalid input path")?,
            "-c:v", "copy", // Copy video stream
            "-an", // Remove audio
            "-y", // Overwrite output file
            output_path.to_str().context("Invalid output path")?,
        ])
        .output()
        .await
        .context("Failed to execute ffmpeg")?;

    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ffmpeg failed to extract video: {}", error);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frame_rate() {
        assert_eq!(parse_frame_rate("30/1").unwrap(), 30.0);
        assert_eq!(parse_frame_rate("25/1").unwrap(), 25.0);
        assert_eq!(parse_frame_rate("60000/1001").unwrap(), 60000.0 / 1001.0);
    }

    #[test]
    fn test_validate_video_file_extension() {
        use std::fs::File;
        use tempfile::tempdir;

        let temp_dir = tempdir().unwrap();
        
        // Test valid extension
        let valid_path = temp_dir.path().join("test.mp4");
        File::create(&valid_path).unwrap();
        assert!(validate_video_file(&valid_path).is_ok());

        // Test invalid extension
        let invalid_path = temp_dir.path().join("test.txt");
        File::create(&invalid_path).unwrap();
        assert!(validate_video_file(&invalid_path).is_err());
    }
}