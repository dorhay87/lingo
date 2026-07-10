use std::sync::Arc;
use std::time::Duration;

/// Swappable wrapper around selected-text capture. The concrete impl uses the
/// get-selected-text crate (UI Automation with clipboard-simulation fallback).
pub trait SelectionCapture: Send + Sync {
    /// Returns the current selection, or None when nothing usable is selected.
    fn capture(&self) -> Option<String>;
}

pub struct SystemSelectionCapture;

impl SelectionCapture for SystemSelectionCapture {
    fn capture(&self) -> Option<String> {
        let started = std::time::Instant::now();
        let result = get_selected_text::get_selected_text();
        let elapsed = started.elapsed().as_millis();
        match result {
            Ok(text) => {
                let trimmed = text.trim();
                log::debug!("selection capture: {} chars in {elapsed}ms", trimmed.len());
                (!trimmed.is_empty()).then(|| trimmed.to_string())
            }
            Err(e) => {
                log::debug!("selection capture failed in {elapsed}ms: {e}");
                None
            }
        }
    }
}

/// Run capture on a dedicated thread with a hard deadline. On timeout the
/// popup opens empty instead of lagging; the straggler thread's result is
/// dropped.
pub async fn capture_with_timeout(
    capture: Arc<dyn SelectionCapture>,
    timeout: Duration,
) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    std::thread::spawn(move || {
        let _ = tx.send(capture.capture());
    });
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(result)) => result,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeCapture {
        delay: Duration,
        result: Option<&'static str>,
    }

    impl SelectionCapture for FakeCapture {
        fn capture(&self) -> Option<String> {
            std::thread::sleep(self.delay);
            self.result.map(String::from)
        }
    }

    #[tokio::test]
    async fn returns_selection_when_fast() {
        let capture = Arc::new(FakeCapture {
            delay: Duration::ZERO,
            result: Some("hallo"),
        });
        let got = capture_with_timeout(capture, Duration::from_millis(300)).await;
        assert_eq!(got.as_deref(), Some("hallo"));
    }

    #[tokio::test]
    async fn times_out_to_none_instead_of_lagging() {
        let capture = Arc::new(FakeCapture {
            delay: Duration::from_secs(2),
            result: Some("too late"),
        });
        let start = std::time::Instant::now();
        let got = capture_with_timeout(capture, Duration::from_millis(100)).await;
        assert!(got.is_none());
        assert!(start.elapsed() < Duration::from_millis(500));
    }

    #[tokio::test]
    async fn empty_capture_is_none() {
        let capture = Arc::new(FakeCapture {
            delay: Duration::ZERO,
            result: None,
        });
        let got = capture_with_timeout(capture, Duration::from_millis(300)).await;
        assert!(got.is_none());
    }
}
