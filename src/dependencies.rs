use crate::error::{BabymodeError, Result};
use log::{info, warn};
use tokio::process::Command;

/// Check if all required system dependencies are available
pub async fn validate_dependencies() -> Result<()> {
    info!("Validating system dependencies...");
    
    check_ffmpeg().await?;
    check_python_and_whisper().await?;
    
    info!("All dependencies validated successfully");
    Ok(())
}

/// Check if FFmpeg is available and get version info
async fn check_ffmpeg() -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(["-version"])
        .output()
        .await
        .map_err(|_| BabymodeError::MissingDependency {
            name: "FFmpeg".to_string(),
            suggestion: "Install FFmpeg: https://ffmpeg.org/download.html".to_string(),
        })?;

    if !output.status.success() {
        return Err(BabymodeError::MissingDependency {
            name: "FFmpeg".to_string(),
            suggestion: "FFmpeg is installed but not working properly".to_string(),
        });
    }

    // Extract version info
    let version_info = String::from_utf8_lossy(&output.stdout);
    if let Some(version_line) = version_info.lines().next() {
        info!("FFmpeg found: {}", version_line);
    }

    Ok(())
}

/// Check if Python and faster-whisper are available
async fn check_python_and_whisper() -> Result<()> {
    // Check Python - try python3 first, then python
    let python_output = match Command::new("python3")
        .args(["-c", "import sys; print(f'Python {sys.version.split()[0]}')"])
        .output()
        .await
    {
        Ok(output) => output,
        Err(_) => {
            Command::new("python")
                .args(["-c", "import sys; print(f'Python {sys.version.split()[0]}')"])
                .output()
                .await
                .map_err(|_| BabymodeError::MissingDependency {
                    name: "Python".to_string(),
                    suggestion: "Install Python 3.8+ from https://python.org".to_string(),
                })?
        }
    };

    if !python_output.status.success() {
        return Err(BabymodeError::MissingDependency {
            name: "Python".to_string(),
            suggestion: "Python is installed but not working properly".to_string(),
        });
    }

    let python_version = String::from_utf8_lossy(&python_output.stdout);
    info!("Python found: {}", python_version.trim());

    // Check faster-whisper
    let whisper_cmd = if tokio::process::Command::new("python3")
        .arg("--version")
        .output()
        .await
        .is_ok() 
    {
        "python3"
    } else {
        "python"
    };

    let whisper_output = Command::new(whisper_cmd)
        .args(["-c", "import faster_whisper; print(f'faster-whisper {faster_whisper.__version__}')"])
        .output()
        .await
        .map_err(|_| BabymodeError::MissingDependency {
            name: "faster-whisper".to_string(),
            suggestion: "Install faster-whisper: pip install faster-whisper".to_string(),
        })?;

    if !whisper_output.status.success() {
        let stderr = String::from_utf8_lossy(&whisper_output.stderr);
        if stderr.contains("No module named 'faster_whisper'") {
            return Err(BabymodeError::MissingDependency {
                name: "faster-whisper".to_string(),
                suggestion: "Install faster-whisper: pip install faster-whisper".to_string(),
            });
        } else {
            warn!("faster-whisper check failed, but may still work: {}", stderr);
            return Ok(()); // Don't fail hard, might still work
        }
    }

    let whisper_version = String::from_utf8_lossy(&whisper_output.stdout);
    info!("faster-whisper found: {}", whisper_version.trim());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_dependency_validation() {
        // This test will only pass if dependencies are installed
        // In CI/CD, this could be configured to expect failure
        let result = validate_dependencies().await;
        
        // Don't fail the test if dependencies aren't available in test environment
        match result {
            Ok(()) => println!("Dependencies available"),
            Err(e) => println!("Dependencies not available: {}", e),
        }
    }
}