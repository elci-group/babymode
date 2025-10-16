use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::sleep;

/// Progress tracker for babymode operations
pub struct ProgressTracker {
    multi: Arc<MultiProgress>,
}

impl ProgressTracker {
    pub fn new() -> Self {
        Self {
            multi: Arc::new(MultiProgress::new()),
        }
    }

    /// Create a progress bar for a specific operation
    pub fn create_progress_bar(&self, total: u64, message: &str) -> ProgressBar {
        let pb = self.multi.add(ProgressBar::new(total));
        pb.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed_precise}] [{wide_bar:.cyan/blue}] {pos}/{len} {msg}"
            )
            .unwrap()
            .progress_chars("#>-")
        );
        pb.set_message(message.to_string());
        pb
    }

    /// Create an indeterminate spinner for unknown-duration operations
    pub fn create_spinner(&self, message: &str) -> ProgressBar {
        let pb = self.multi.add(ProgressBar::new_spinner());
        pb.set_style(
            ProgressStyle::with_template("{spinner:.green} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
        );
        pb.set_message(message.to_string());
        pb.enable_steady_tick(Duration::from_millis(100));
        pb
    }

    /// Join all progress bars (blocks until all are finished)
    pub async fn join_all(&self) {
        // Simple implementation - MultiProgress doesn't have is_finished in this version
        // This is mainly for future use when we need to wait for multiple operations
        sleep(Duration::from_millis(100)).await;
    }
}

impl Default for ProgressTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Wrapper for operations with progress tracking
pub struct ProgressOperation {
    pub tracker: ProgressTracker,
    pub enabled: bool,
}

impl ProgressOperation {
    pub fn new(enabled: bool) -> Self {
        Self {
            tracker: ProgressTracker::new(),
            enabled,
        }
    }

    /// Execute an operation with a progress bar if enabled
    pub async fn with_progress<F, T>(&self, 
        total: u64, 
        message: &str, 
        mut operation: F
    ) -> T 
    where 
        F: FnMut(Option<&ProgressBar>) -> T,
    {
        if self.enabled {
            let pb = self.tracker.create_progress_bar(total, message);
            let result = operation(Some(&pb));
            pb.finish_with_message(format!("✓ {}", message));
            result
        } else {
            operation(None)
        }
    }

    /// Execute an operation with a spinner if enabled
    pub async fn with_spinner<F, T>(&self, 
        message: &str, 
        mut operation: F
    ) -> T 
    where 
        F: FnMut(Option<&ProgressBar>) -> T,
    {
        if self.enabled {
            let pb = self.tracker.create_spinner(message);
            let result = operation(Some(&pb));
            pb.finish_with_message(format!("✓ {}", message));
            result
        } else {
            operation(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_progress_tracker_creation() {
        let tracker = ProgressTracker::new();
        let pb = tracker.create_progress_bar(100, "Test operation");
        
        // Simulate some progress
        for i in 0..=100 {
            pb.set_position(i);
            if i % 20 == 0 {
                sleep(Duration::from_millis(10)).await;
            }
        }
        
        pb.finish_with_message("Test completed");
        assert!(pb.is_finished());
    }

    #[tokio::test]
    async fn test_progress_operation() {
        let progress = ProgressOperation::new(true);
        
        let result = progress.with_spinner("Test operation", |pb| {
            if let Some(pb) = pb {
                assert!(!pb.is_finished());
            }
            42
        }).await;
        
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn test_disabled_progress() {
        let progress = ProgressOperation::new(false);
        
        // This should work without creating any progress bars
        let result = progress.with_spinner("Test", |pb| {
            assert!(pb.is_none());
            "success"
        }).await;
        
        assert_eq!(result, "success");
    }
}