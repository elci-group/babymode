use anyhow::{Context, Result};
use log::{debug, info, warn};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use tempfile::NamedTempFile;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::audio::AudioSegment;
use crate::Config;

/// Word detection result with timing and confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordDetection {
    pub word: String,
    pub start_time: f64,
    pub end_time: f64,
    pub confidence: f64,
    pub is_swear: bool,
}

impl WordDetection {
    pub fn to_audio_segment(&self) -> AudioSegment {
        AudioSegment::new(self.start_time, self.end_time)
    }
}

/// Whisper transcription segment
#[derive(Debug, Deserialize)]
struct WhisperSegment {
    start: f64,
    end: f64,
    text: String,
    words: Option<Vec<WhisperWord>>,
}

/// Individual word from Whisper with timing
#[derive(Debug, Deserialize)]
struct WhisperWord {
    word: String,
    start: f64,
    end: f64,
    probability: f64,
}

/// Detect swear words in audio using faster-whisper via Python
pub async fn detect_swear_words(audio_path: &Path, config: &Config) -> Result<Vec<WordDetection>> {
    info!("Detecting swear words using faster-whisper model: {}", config.whisper_model.as_str());

    // Create temporary Python script for faster-whisper
    let python_script = create_whisper_script()?;
    
    // Run faster-whisper transcription
    let transcription_result = run_whisper_transcription(
        &python_script,
        audio_path,
        config.whisper_model.as_str(),
    ).await?;

    // Parse the transcription results
    let segments: Vec<WhisperSegment> = serde_json::from_str(&transcription_result)
        .context("Failed to parse whisper transcription results")?;

    // Extract words and check for swear words
    let mut detections = Vec::new();
    
    for segment in segments {
        if let Some(words) = segment.words {
            for word in words {
                let cleaned_word = clean_word(&word.word);
                let is_swear = is_swear_word(&cleaned_word, &config.swear_words);
                
                let detection = WordDetection {
                    word: cleaned_word.clone(),
                    start_time: word.start,
                    end_time: word.end,
                    confidence: word.probability,
                    is_swear,
                };
                
                if is_swear {
                    info!("Detected swear word: '{}' at {:.2}s-{:.2}s (confidence: {:.2})", 
                          cleaned_word, word.start, word.end, word.probability);
                }
                
                detections.push(detection);
            }
        } else {
            // Fallback: analyze segment text if individual words aren't available
            let words = segment.text.split_whitespace();
            let segment_duration = segment.end - segment.start;
            let word_count = words.clone().count() as f64;
            
            for (i, word) in words.enumerate() {
                let cleaned_word = clean_word(word);
                let is_swear = is_swear_word(&cleaned_word, &config.swear_words);
                
                // Estimate word timing based on position in segment
                let word_start = segment.start + (i as f64 / word_count) * segment_duration;
                let word_end = segment.start + ((i + 1) as f64 / word_count) * segment_duration;
                
                let detection = WordDetection {
                    word: cleaned_word.clone(),
                    start_time: word_start,
                    end_time: word_end,
                    confidence: 0.8, // Default confidence for segment-based detection
                    is_swear,
                };
                
                if is_swear {
                    warn!("Detected swear word (estimated timing): '{}' at {:.2}s-{:.2}s", 
                          cleaned_word, word_start, word_end);
                }
                
                detections.push(detection);
            }
        }
    }

    let swear_detections: Vec<WordDetection> = detections.into_iter()
        .filter(|d| d.is_swear)
        .collect();

    info!("Found {} swear word occurrences", swear_detections.len());
    Ok(swear_detections)
}

/// Create a temporary Python script for faster-whisper
fn create_whisper_script() -> Result<NamedTempFile> {
    let script_content = r#"
import sys
import json
import os
from faster_whisper import WhisperModel

def transcribe_audio(model_size, audio_path):
    try:
        # Check if audio file exists
        if not os.path.exists(audio_path):
            raise FileNotFoundError(f"Audio file not found: {audio_path}")
        
        print(f"Loading model: {model_size}", file=sys.stderr)
        # Load the model
        model = WhisperModel(model_size, device="cpu", compute_type="int8")
        
        print(f"Transcribing: {audio_path}", file=sys.stderr)
        # Transcribe with word-level timestamps
        segments, info = model.transcribe(
            audio_path,
            word_timestamps=True,
            language="en"  # Assuming English, could be auto-detected
        )
        
        # Convert segments to serializable format
        result = []
        for segment in segments:
            segment_data = {
                "start": segment.start,
                "end": segment.end,
                "text": segment.text,
                "words": []
            }
            
            if hasattr(segment, 'words') and segment.words:
                for word in segment.words:
                    word_data = {
                        "word": word.word,
                        "start": word.start,
                        "end": word.end,
                        "probability": word.probability
                    }
                    segment_data["words"].append(word_data)
            
            result.append(segment_data)
        
        print(f"Transcription complete: {len(result)} segments", file=sys.stderr)
        return result
    
    except Exception as e:
        print(f"Error in transcription: {e}", file=sys.stderr)
        return []

if __name__ == "__main__":
    if len(sys.argv) != 3:
        print("Usage: python script.py <model_size> <audio_path>", file=sys.stderr)
        sys.exit(1)
    
    model_size = sys.argv[1]
    audio_path = sys.argv[2]
    
    result = transcribe_audio(model_size, audio_path)
    print(json.dumps(result, indent=2))
"#;

    let mut temp_file = NamedTempFile::new()
        .context("Failed to create temporary Python script")?;
    
    temp_file.write_all(script_content.as_bytes())
        .context("Failed to write Python script")?;
    
    temp_file.flush().context("Failed to flush Python script")?;
    
    Ok(temp_file)
}

/// Run the whisper transcription using Python
async fn run_whisper_transcription(
    script_path: &NamedTempFile,
    audio_path: &Path,
    model_size: &str,
) -> Result<String> {
    // Ensure the audio file exists
    if !audio_path.exists() {
        anyhow::bail!("Audio file does not exist: {:?}", audio_path);
    }

    debug!("Running whisper transcription: script={:?}, audio={:?}, model={}", 
           script_path.path(), audio_path, model_size);

    let mut child = Command::new("python3")
        .args([
            script_path.path().to_str().context("Invalid script path")?,
            model_size,
            audio_path.to_str().context("Invalid audio path")?,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Failed to spawn Python process. Make sure Python 3 and faster-whisper are installed.")?;

    let stdout = child.stdout.take().context("Failed to get stdout")?;
    let stderr = child.stderr.take().context("Failed to get stderr")?;

    // Read stdout
    let mut stdout_reader = BufReader::new(stdout);
    let mut output = String::new();
    let mut line = String::new();
    
    while stdout_reader.read_line(&mut line).await? > 0 {
        output.push_str(&line);
        line.clear();
    }

    // Read stderr for error reporting
    let mut stderr_reader = BufReader::new(stderr);
    let mut error_output = String::new();
    line.clear();
    
    while stderr_reader.read_line(&mut line).await? > 0 {
        error_output.push_str(&line);
        line.clear();
    }

    // Wait for the process to finish
    let status = child.wait().await.context("Failed to wait for Python process")?;

    if !status.success() {
        anyhow::bail!("Whisper transcription failed: {}", error_output);
    }

    if !error_output.is_empty() {
        warn!("Whisper stderr output: {}", error_output);
    }

    Ok(output)
}

/// Clean a word by removing punctuation and converting to lowercase
fn clean_word(word: &str) -> String {
    // Remove leading/trailing whitespace and punctuation
    let cleaned = word.trim()
        .trim_matches(|c: char| c.is_ascii_punctuation())
        .to_lowercase();
    
    cleaned
}

/// Check if a word is in the swear words list
fn is_swear_word(word: &str, swear_words: &[String]) -> bool {
    if word.is_empty() || word.len() < 2 {
        return false; // Ignore single letters
    }
    
    let word_lower = word.to_lowercase();
    
    // Skip common false positives
    if matches!(word_lower.as_str(), "i" | "a" | "he" | "it" | "in" | "is" | "to" | "or" | "as" | "be" | "we" | "on" | "so" | "up" | "an" | "my" | "at" | "go" | "do" | "if" | "no" | "me" | "us" | "oh") {
        return false;
    }
    
    // Direct match
    if swear_words.contains(&word_lower) {
        return true;
    }
    
    // Check for partial matches (but only for words >= 4 chars to avoid false positives)
    if word.len() >= 4 {
        for swear in swear_words {
            if swear.len() >= 4 && (word_lower.contains(swear) || swear.contains(&word_lower)) {
                return true;
            }
        }
    }
    
    // Check common variations (e.g., "sh*t", "f**k")
    for swear in swear_words {
        if is_censored_variation(&word_lower, swear) {
            return true;
        }
    }
    
    false
}

/// Check if a word is a censored variation of a swear word
fn is_censored_variation(word: &str, swear: &str) -> bool {
    if word.len() != swear.len() {
        return false;
    }
    
    let mut matches = 0;
    let chars1: Vec<char> = word.chars().collect();
    let chars2: Vec<char> = swear.chars().collect();
    
    for (c1, c2) in chars1.iter().zip(chars2.iter()) {
        if c1 == c2 {
            matches += 1;
        } else if *c1 == '*' || *c1 == '#' || *c1 == '@' {
            // Common censoring characters
            matches += 1;
        }
    }
    
    // If more than half the characters match, it's likely a censored version
    matches > word.len() / 2
}

/// Merge overlapping or adjacent word detections into segments
pub fn merge_detections(detections: Vec<WordDetection>, merge_gap: f64) -> Vec<AudioSegment> {
    if detections.is_empty() {
        return Vec::new();
    }
    
    let mut sorted_detections = detections;
    sorted_detections.sort_by(|a, b| a.start_time.partial_cmp(&b.start_time).unwrap());
    
    let mut segments = Vec::new();
    let mut current_start = sorted_detections[0].start_time;
    let mut current_end = sorted_detections[0].end_time;
    
    for detection in sorted_detections.iter().skip(1) {
        if detection.start_time <= current_end + merge_gap {
            // Extend current segment
            current_end = current_end.max(detection.end_time);
        } else {
            // Create new segment
            segments.push(AudioSegment::new(current_start, current_end));
            current_start = detection.start_time;
            current_end = detection.end_time;
        }
    }
    
    // Add the final segment
    segments.push(AudioSegment::new(current_start, current_end));
    
    debug!("Merged {} detections into {} segments", sorted_detections.len(), segments.len());
    segments
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_word() {
        assert_eq!(clean_word("  hello!  "), "hello");
        assert_eq!(clean_word("fuck,"), "fuck");
        assert_eq!(clean_word("'damn'"), "damn");
        assert_eq!(clean_word("SHIT"), "shit");
    }

    #[test]
    fn test_is_swear_word() {
        let swear_words = vec!["fuck".to_string(), "shit".to_string()];
        
        assert!(is_swear_word("fuck", &swear_words));
        assert!(is_swear_word("fucking", &swear_words));
        assert!(is_swear_word("shit", &swear_words));
        assert!(!is_swear_word("hello", &swear_words));
        assert!(!is_swear_word("", &swear_words));
    }

    #[test]
    fn test_is_censored_variation() {
        assert!(is_censored_variation("f**k", "fuck"));
        assert!(is_censored_variation("s#it", "shit"));
        assert!(is_censored_variation("f@ck", "fuck"));
        assert!(!is_censored_variation("hello", "fuck"));
        assert!(!is_censored_variation("f*ck", "hello"));
    }

    #[test]
    fn test_merge_detections() {
        let detections = vec![
            WordDetection {
                word: "fuck".to_string(),
                start_time: 10.0,
                end_time: 10.5,
                confidence: 0.9,
                is_swear: true,
            },
            WordDetection {
                word: "that".to_string(),
                start_time: 10.6,
                end_time: 11.0,
                confidence: 0.8,
                is_swear: false,
            },
            WordDetection {
                word: "shit".to_string(),
                start_time: 11.1,
                end_time: 11.5,
                confidence: 0.95,
                is_swear: true,
            },
        ];
        
        let swear_detections: Vec<WordDetection> = detections.into_iter()
            .filter(|d| d.is_swear)
            .collect();
        
        let segments = merge_detections(swear_detections, 1.0);
        assert_eq!(segments.len(), 1); // Should merge into one segment
        assert_eq!(segments[0].start_time, 10.0);
        assert_eq!(segments[0].end_time, 11.5);
    }
}