/*
Implement small futures that wake their executor before returning Pending.
*/

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Countdown Future: decrements count by 1 each time it's polled,
/// returns `"liftoff!"` when count reaches 0.
pub struct CountDown {
    pub count: u32,
}

impl CountDown {
    pub fn new(count: u32) -> Self {
        Self { count }
    }
}

impl Future for CountDown {
    type Output = &'static str;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.count == 0 {
            Poll::Ready("liftoff!")
        } else {
            this.count -= 1;
            // Pending must arrange another poll; no external event wakes this future.
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Yield-only-once Future: first poll returns Pending, second returns Ready(()).
/// This is the minimal example of an asynchronous state machine.
pub struct YieldOnce {
    yielded: bool,
}

impl YieldOnce {
    pub fn new() -> Self {
        Self { yielded: false }
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.yielded {
            Poll::Ready(())
        } else {
            this.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Wake, Waker};

    struct WakeCount(AtomicUsize);

    impl Wake for WakeCount {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn test_countdown_poll_and_wake_contract() {
        let wake_count = Arc::new(WakeCount(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&wake_count));
        let mut context = Context::from_waker(&waker);
        let mut countdown = CountDown::new(2);

        assert_eq!(Pin::new(&mut countdown).poll(&mut context), Poll::Pending);
        assert_eq!(countdown.count, 1);
        assert_eq!(Pin::new(&mut countdown).poll(&mut context), Poll::Pending);
        assert_eq!(countdown.count, 0);
        assert_eq!(
            Pin::new(&mut countdown).poll(&mut context),
            Poll::Ready("liftoff!")
        );
        assert_eq!(wake_count.0.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn test_yield_once_poll_and_wake_contract() {
        let wake_count = Arc::new(WakeCount(AtomicUsize::new(0)));
        let waker = Waker::from(Arc::clone(&wake_count));
        let mut context = Context::from_waker(&waker);
        let mut future = YieldOnce::new();

        assert_eq!(Pin::new(&mut future).poll(&mut context), Poll::Pending);
        assert_eq!(Pin::new(&mut future).poll(&mut context), Poll::Ready(()));
        assert_eq!(wake_count.0.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn test_countdown_zero() {
        let result = CountDown::new(0).await;
        assert_eq!(result, "liftoff!");
    }

    #[tokio::test]
    async fn test_countdown_three() {
        let result = CountDown::new(3).await;
        assert_eq!(result, "liftoff!");
    }

    #[tokio::test]
    async fn test_yield_once() {
        YieldOnce::new().await;
    }

    #[tokio::test]
    async fn test_countdown_large() {
        let result = CountDown::new(100).await;
        assert_eq!(result, "liftoff!");
    }
}
