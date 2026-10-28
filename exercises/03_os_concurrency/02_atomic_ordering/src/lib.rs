/* Publish channel data and initialize a value exactly once. */

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

pub struct FlagChannel {
    data: AtomicU32,
    ready: AtomicBool,
}

impl FlagChannel {
    pub const fn new() -> Self {
        Self {
            data: AtomicU32::new(0),
            ready: AtomicBool::new(false),
        }
    }

    pub fn produce(&self, value: u32) {
        self.data.store(value, Ordering::Relaxed);
        // The flag publishes all writes preceding it to the acquiring consumer.
        self.ready.store(true, Ordering::Release);
    }

    pub fn consume(&self) -> u32 {
        while !self.ready.load(Ordering::Acquire) {
            std::hint::spin_loop();
        }
        self.data.load(Ordering::Relaxed)
    }

    /// Reset only after the producer and consumer have finished this round.
    pub fn reset(&self) {
        self.ready.store(false, Ordering::Relaxed);
        self.data.store(0, Ordering::Relaxed);
    }
}

pub struct OnceCell {
    claimed: AtomicBool,
    initialized: AtomicBool,
    value: AtomicU32,
}

impl OnceCell {
    pub const fn new() -> Self {
        Self {
            claimed: AtomicBool::new(false),
            initialized: AtomicBool::new(false),
            value: AtomicU32::new(0),
        }
    }

    pub fn init(&self, val: u32) -> bool {
        if self
            .claimed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        self.value.store(val, Ordering::SeqCst);
        // Claiming initialization and publishing a completed value are separate states.
        self.initialized.store(true, Ordering::SeqCst);
        true
    }

    pub fn get(&self) -> Option<u32> {
        if self.initialized.load(Ordering::SeqCst) {
            Some(self.value.load(Ordering::SeqCst))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn a_claim_does_not_publish_an_unwritten_value() {
        let cell = OnceCell::new();
        cell.claimed.store(true, Ordering::SeqCst);
        assert_eq!(cell.get(), None);
        cell.value.store(73, Ordering::SeqCst);
        cell.initialized.store(true, Ordering::SeqCst);
        assert_eq!(cell.get(), Some(73));
    }

    #[test]
    fn concurrent_readers_only_observe_the_winning_value() {
        let cell = Arc::new(OnceCell::new());
        let start = Arc::new(Barrier::new(9));
        let threads: Vec<_> = (1..=8)
            .map(|value| {
                let cell = cell.clone();
                let start = start.clone();
                std::thread::spawn(move || {
                    start.wait();
                    let won = cell.init(value);
                    let published = loop {
                        if let Some(value) = cell.get() {
                            break value;
                        }
                        std::hint::spin_loop();
                    };
                    assert_ne!(published, 0);
                    (won, value, published)
                })
            })
            .collect();
        start.wait();
        let results: Vec<_> = threads.into_iter().map(|t| t.join().unwrap()).collect();
        let winners: Vec<_> = results.iter().filter(|result| result.0).collect();
        assert_eq!(winners.len(), 1);
        assert!(results.iter().all(|result| result.2 == winners[0].1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_flag_channel() {
        let ch = Arc::new(FlagChannel::new());
        let ch2 = Arc::clone(&ch);

        let producer = thread::spawn(move || {
            ch2.produce(42);
        });

        let consumer = thread::spawn(move || ch.consume());

        producer.join().unwrap();
        let val = consumer.join().unwrap();
        assert_eq!(val, 42);
    }

    #[test]
    fn test_flag_channel_large_value() {
        let ch = Arc::new(FlagChannel::new());
        let ch2 = Arc::clone(&ch);

        let producer = thread::spawn(move || {
            ch2.produce(0xDEAD_BEEF);
        });

        let val = ch.consume();
        producer.join().unwrap();
        assert_eq!(val, 0xDEAD_BEEF);
    }

    #[test]
    fn test_once_cell_init_once() {
        let cell = OnceCell::new();
        assert!(cell.init(42));
        assert!(!cell.init(100));
        assert_eq!(cell.get(), Some(42));
    }

    #[test]
    fn test_once_cell_not_initialized() {
        let cell = OnceCell::new();
        assert_eq!(cell.get(), None);
    }

    #[test]
    fn test_once_cell_concurrent() {
        let cell = Arc::new(OnceCell::new());
        let mut handles = vec![];

        for i in 0..10 {
            let c = Arc::clone(&cell);
            handles.push(thread::spawn(move || c.init(i)));
        }

        let results: Vec<bool> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        // Exactly one thread initializes successfully
        assert_eq!(results.iter().filter(|&&r| r).count(), 1);
        assert!(cell.get().is_some());
    }
}
