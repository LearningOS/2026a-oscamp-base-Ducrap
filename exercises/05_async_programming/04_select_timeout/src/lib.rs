/*
Apply deadlines and race futures, dropping unfinished operations.
*/

use std::future::Future;
use tokio::time::Duration;

/// Async operation with timeout.
/// If `future` completes within `timeout_ms` milliseconds, returns Some(result).
/// Otherwise returns None.
///
/// Hint: Use `tokio::select!` or `tokio::time::timeout`.
pub async fn with_timeout<F, T>(future: F, timeout_ms: u64) -> Option<T>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(Duration::from_millis(timeout_ms), future)
        .await
        .ok()
}

/// Race two async tasks, return the result of whichever finishes first.
///
/// Hint: Use `tokio::select!` macro.
pub async fn race<F1, F2, T>(f1: F1, f2: F2) -> T
where
    F1: Future<Output = T>,
    F2: Future<Output = T>,
{
    tokio::select! {
        result = f1 => result,
        result = f2 => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use tokio::time::sleep;

    struct PendingUntilDropped(Arc<AtomicBool>);

    impl Future for PendingUntilDropped {
        type Output = i32;

        fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<i32> {
            Poll::Pending
        }
    }

    impl Drop for PendingUntilDropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    #[tokio::test]
    async fn test_timeout_drops_unfinished_future() {
        let dropped = Arc::new(AtomicBool::new(false));
        let result = with_timeout(PendingUntilDropped(Arc::clone(&dropped)), 1).await;
        assert_eq!(result, None);
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_race_drops_losing_future() {
        let dropped = Arc::new(AtomicBool::new(false));
        let result = race(async { 7 }, PendingUntilDropped(Arc::clone(&dropped))).await;
        assert_eq!(result, 7);
        assert!(dropped.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_timeout_success() {
        let result = with_timeout(async { 42 }, 100).await;
        assert_eq!(result, Some(42));
    }

    #[tokio::test]
    async fn test_timeout_expired() {
        let result = with_timeout(
            async {
                sleep(Duration::from_millis(200)).await;
                42
            },
            50,
        )
        .await;
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_race_first_wins() {
        let result = race(
            async {
                sleep(Duration::from_millis(10)).await;
                "fast"
            },
            async {
                sleep(Duration::from_millis(200)).await;
                "slow"
            },
        )
        .await;
        assert_eq!(result, "fast");
    }

    #[tokio::test]
    async fn test_race_second_wins() {
        let result = race(
            async {
                sleep(Duration::from_millis(200)).await;
                "slow"
            },
            async {
                sleep(Duration::from_millis(10)).await;
                "fast"
            },
        )
        .await;
        assert_eq!(result, "fast");
    }
}
