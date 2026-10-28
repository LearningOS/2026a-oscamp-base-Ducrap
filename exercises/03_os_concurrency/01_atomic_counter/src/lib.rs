/* Atomic counter operations and a retrying compare-and-swap update. */

use std::sync::atomic::{AtomicU64, Ordering};

pub struct AtomicCounter {
    value: AtomicU64,
}

impl AtomicCounter {
    pub const fn new(init: u64) -> Self {
        Self {
            value: AtomicU64::new(init),
        }
    }

    pub fn increment(&self) -> u64 {
        self.value.fetch_add(1, Ordering::Relaxed)
    }

    pub fn decrement(&self) -> u64 {
        self.value.fetch_sub(1, Ordering::Relaxed)
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    pub fn compare_and_swap(&self, expected: u64, new_val: u64) -> Result<u64, u64> {
        self.value
            .compare_exchange(expected, new_val, Ordering::AcqRel, Ordering::Acquire)
    }

    pub fn fetch_multiply(&self, multiplier: u64) -> u64 {
        let mut current = self.get();
        loop {
            match self.compare_and_swap(current, current.wrapping_mul(multiplier)) {
                Ok(previous) => return previous,
                Err(actual) => current = actual,
            }
        }
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn multiplication_retries_and_wraps_like_atomic_add() {
        let counter = Arc::new(AtomicCounter::new(1));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let counter = counter.clone();
                std::thread::spawn(move || {
                    for _ in 0..4 {
                        counter.fetch_multiply(2);
                    }
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(counter.get(), 1 << 32);
        let counter = AtomicCounter::new(u64::MAX);
        assert_eq!(counter.fetch_multiply(2), u64::MAX);
        assert_eq!(counter.get(), u64::MAX - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_basic_ops() {
        let c = AtomicCounter::new(0);
        assert_eq!(c.increment(), 0);
        assert_eq!(c.increment(), 1);
        assert_eq!(c.get(), 2);
        assert_eq!(c.decrement(), 2);
        assert_eq!(c.get(), 1);
    }

    #[test]
    fn test_cas_success() {
        let c = AtomicCounter::new(10);
        assert_eq!(c.compare_and_swap(10, 20), Ok(10));
        assert_eq!(c.get(), 20);
    }

    #[test]
    fn test_cas_failure() {
        let c = AtomicCounter::new(10);
        assert_eq!(c.compare_and_swap(5, 20), Err(10));
        assert_eq!(c.get(), 10);
    }

    #[test]
    fn test_fetch_multiply() {
        let c = AtomicCounter::new(3);
        let old = c.fetch_multiply(4);
        assert_eq!(old, 3);
        assert_eq!(c.get(), 12);
    }

    #[test]
    fn test_concurrent_increment() {
        let counter = Arc::new(AtomicCounter::new(0));
        let mut handles = vec![];

        for _ in 0..10 {
            let c = Arc::clone(&counter);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    c.increment();
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(counter.get(), 10000);
    }
}
