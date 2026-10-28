/*
Share counters and collections between threads using Arc and Mutex.
*/

use std::sync::{Arc, Mutex};
use std::thread;

/// Increment a counter concurrently using `n_threads` threads.
/// Each thread increments the counter `count_per_thread` times.
/// Returns the final counter value.
///
/// Hint: Use `Arc<Mutex<usize>>` as the shared counter.
pub fn concurrent_counter(n_threads: usize, count_per_thread: usize) -> usize {
    let counter = Arc::new(Mutex::new(0));
    let mut handles = Vec::with_capacity(n_threads);
    for _ in 0..n_threads {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            for _ in 0..count_per_thread {
                *counter.lock().expect("counter mutex poisoned") += 1;
            }
        }));
    }
    for handle in handles {
        handle.join().expect("counter thread panicked");
    }
    let total = *counter.lock().expect("counter mutex poisoned");
    total
}

/// Add elements to a shared vector concurrently using multiple threads.
/// Each thread pushes its own id (0..n_threads) to the vector.
/// Returns the sorted vector.
///
/// Hint: Use `Arc<Mutex<Vec<usize>>>`.
pub fn concurrent_collect(n_threads: usize) -> Vec<usize> {
    let values = Arc::new(Mutex::new(Vec::with_capacity(n_threads)));
    let mut handles = Vec::with_capacity(n_threads);
    for id in 0..n_threads {
        let values = Arc::clone(&values);
        handles.push(thread::spawn(move || {
            values.lock().expect("collection mutex poisoned").push(id);
        }));
    }
    for handle in handles {
        handle.join().expect("collection thread panicked");
    }
    let mut values = values.lock().expect("collection mutex poisoned");
    values.sort_unstable();
    std::mem::take(&mut *values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_counter_single_thread() {
        assert_eq!(concurrent_counter(1, 100), 100);
    }

    #[test]
    fn test_counter_multi_thread() {
        assert_eq!(concurrent_counter(10, 100), 1000);
    }

    #[test]
    fn test_counter_zero() {
        assert_eq!(concurrent_counter(5, 0), 0);
    }

    #[test]
    fn test_collect() {
        let result = concurrent_collect(5);
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_collect_single() {
        assert_eq!(concurrent_collect(1), vec![0]);
    }
}
