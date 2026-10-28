/*
Create and join threads, borrow scoped data, and isolate thread-local state.
*/

use std::cell::RefCell;
use std::thread;
use std::time::Duration;

// ============================================================================
// Example Code: Advanced Thread Patterns
// ============================================================================
// The following examples illustrate additional thread‑related concepts that are
// useful in real‑world Rust concurrent programming.

/// Example: Handling thread panic.
///
/// `join()` returns a `Result`. If the thread panics, the `Result` is an `Err`.
/// This demonstrates how to catch and handle a panic from a spawned thread.
///
/// ```rust
/// use std::thread;
///
/// fn panic_handling_example() {
///     let handle = thread::spawn(|| {
///         // Simulate a panic
///         panic!("Thread panicked!");
///     });
///
///     match handle.join() {
///         Ok(_) => println!("Thread completed successfully."),
///         Err(e) => println!("Thread panicked: {:?}", e),
///     }
/// }
/// ```
///
/// In contrast, the exercises below use `unwrap()` for simplicity, assuming
/// that the threads never panic.

/// Example: Named thread and custom stack size.
///
/// Using `thread::Builder` you can assign a name to a thread (helpful for
/// debugging) and set its stack size.
///
/// ```rust
/// use std::thread;
///
/// fn named_thread_example() {
///     let builder = thread::Builder::new()
///         .name("my-worker".into())
///         .stack_size(32 * 1024); // 32 KiB
///
///     let handle = builder.spawn(|| {
///         println!("Hello from thread: {:?}", thread::current().name());
///         42
///     }).unwrap();
///
///     let result = handle.join().unwrap();
///     println!("Thread returned: {}", result);
/// }
/// ```

/// Example: Scoped threads (Rust 1.63+).
///
/// Scoped threads allow borrowing stack data without moving ownership.
/// The threads are guaranteed to finish before the scope ends, so references
/// remain valid.
///
/// ```rust
/// use std::thread;
///
/// fn scoped_thread_example() {
///     let a = vec![1, 2, 3];
///     let b = vec![4, 5, 6];
///
///     let (sum_a, sum_b) = thread::scope(|s| {
///         let h1 = s.spawn(|| a.iter().sum::<i32>());
///         let h2 = s.spawn(|| b.iter().sum::<i32>());
///         (h1.join().unwrap(), h2.join().unwrap())
///     });
///
///     // `a` and `b` are still accessible here.
///     println!("sum_a = {}, sum_b = {}", sum_a, sum_b);
/// }
/// ```

/// Example: Thread‑local storage.
///
/// Each thread gets its own independent copy of a `thread_local!` variable.
///
/// ```rust
/// use std::cell::RefCell;
/// use std::thread;
///
/// thread_local! {
///     static THREAD_ID: RefCell<usize> = RefCell::new(0);
/// }
///
/// fn thread_local_example() {
///     THREAD_ID.with(|id| {
///         *id.borrow_mut() = 1;
///     });
///
///     let handle = thread::spawn(|| {
///         THREAD_ID.with(|id| {
///             *id.borrow_mut() = 2;
///         });
///         THREAD_ID.with(|id| println!("Thread local value: {}", *id.borrow()));
///     });
///
///     handle.join().unwrap();
///
///     THREAD_ID.with(|id| println!("Main thread value: {}", *id.borrow()));
/// }
/// ```

// ============================================================================
// Exercise Functions
// ============================================================================

/// Multiply each element of a vector by 2 in a new thread, returning the result vector.
///
/// Hint: Use `thread::spawn` and `move` closure.
pub fn double_in_thread(numbers: Vec<i32>) -> Vec<i32> {
    thread::spawn(move || numbers.into_iter().map(|number| number * 2).collect())
        .join()
        .expect("worker thread panicked")
}

/// Sum two vectors in parallel, returning a tuple of two sums.
///
/// Hint: Create two threads for each vector.
pub fn parallel_sum(a: Vec<i32>, b: Vec<i32>) -> (i32, i32) {
    let first = thread::spawn(move || a.into_iter().sum());
    let second = thread::spawn(move || b.into_iter().sum());
    (
        first.join().expect("first sum thread panicked"),
        second.join().expect("second sum thread panicked"),
    )
}

// ============================================================================
// Advanced Exercise Functions
// ============================================================================

/// Create a named thread that sleeps for the given milliseconds and then returns the input value.
///
/// The thread should be named `"sleeper"`. Use `thread::Builder` to set the name.
/// Inside the thread, call `thread::sleep(Duration::from_millis(ms))` before returning `value`.
///
/// Hint: `thread::sleep` causes the current thread to block; it does not affect other threads.
pub fn named_sleeper(value: i32, ms: u64) -> i32 {
    thread::Builder::new()
        .name("sleeper".into())
        .spawn(move || {
            thread::sleep(Duration::from_millis(ms));
            value
        })
        .expect("could not spawn sleeper thread")
        .join()
        .expect("sleeper thread panicked")
}

thread_local! {
    static THREAD_COUNT: RefCell<usize> = RefCell::new(0);
}

/// Use thread‑local storage to count how many times each thread calls `increment`.
///
/// Define a `thread_local!` static `THREAD_COUNT` of type `RefCell<usize>` initialized to 0.
/// Each call to `increment` should increase the thread‑local count by 1 and return the new value.
///
/// Hint: Use `THREAD_COUNT.with(|cell| { ... })` to access the thread‑local variable.
pub fn increment_thread_local() -> usize {
    THREAD_COUNT.with(|cell| {
        let mut count = cell.borrow_mut();
        *count += 1;
        *count
    })
}

/// Spawn two threads using a **scoped thread** to compute the sum of two slices without moving ownership.
///
/// Use `thread::scope` to allow threads to borrow the slices `&[i32]`.
/// Each thread should compute the sum of its slice, and the function returns `(sum_a, sum_b)`.
///
/// Hint: The slices are references, so you cannot move them into the closure.
/// `thread::scope` guarantees that all spawned threads finish before the scope ends,
/// making the borrow safe.
pub fn scoped_slice_sum(a: &[i32], b: &[i32]) -> (i32, i32) {
    thread::scope(|scope| {
        let first = scope.spawn(|| a.iter().sum());
        let second = scope.spawn(|| b.iter().sum());
        (
            first.join().expect("first scoped thread panicked"),
            second.join().expect("second scoped thread panicked"),
        )
    })
}

/// Handle a possible panic in a spawned thread.
///
/// Spawn a thread that may panic: if `should_panic` is `true`, the thread calls `panic!("oops")`;
/// otherwise it returns `value`.
/// The function should return `Ok(value)` if the thread completed successfully,
/// or `Err(())` if the thread panicked.
///
/// Hint: `join()` returns `Result<Result<i32, Box<dyn Any + Send>>, _>`.
/// You'll need to match the outer `Result` (thread panic) and the inner `Result` (if the thread returns a `Result`).
/// In this exercise, the inner type is just `i32`, not a `Result`.
pub fn handle_panic(value: i32, should_panic: bool) -> Result<i32, ()> {
    thread::spawn(move || {
        if should_panic {
            panic!("oops");
        }
        value
    })
    .join()
    .map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_double_basic() {
        let nums = vec![1, 2, 3, 4, 5];
        assert_eq!(double_in_thread(nums), vec![2, 4, 6, 8, 10]);
    }

    #[test]
    fn test_double_empty() {
        assert_eq!(double_in_thread(vec![]), vec![]);
    }

    #[test]
    fn test_double_negative() {
        assert_eq!(double_in_thread(vec![-1, 0, 1]), vec![-2, 0, 2]);
    }

    #[test]
    fn test_parallel_sum() {
        let a = vec![1, 2, 3];
        let b = vec![10, 20, 30];
        assert_eq!(parallel_sum(a, b), (6, 60));
    }

    #[test]
    fn test_parallel_sum_empty() {
        assert_eq!(parallel_sum(vec![], vec![]), (0, 0));
    }

    // Advanced exercise tests
    #[test]
    fn test_named_sleeper() {
        // The thread should sleep a short time; we just verify it returns the correct value.
        let result = named_sleeper(42, 10); // sleep 10 ms
        assert_eq!(result, 42);
    }

    #[test]
    fn test_thread_local() {
        // Each thread has its own counter, so spawning two threads and calling increment
        // in each should give each thread its own sequence.
        use std::sync::Arc;
        use std::sync::Mutex;

        let counters = Arc::new(Mutex::new(Vec::new()));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let counters = Arc::clone(&counters);
            handles.push(thread::spawn(move || {
                let v1 = increment_thread_local();
                let v2 = increment_thread_local();
                counters.lock().unwrap().push((v1, v2));
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let results = counters.lock().unwrap();
        // Each thread should have counted (1, 2) independently.
        assert_eq!(results.len(), 2);
        assert!(results.contains(&(1, 2)));
    }

    #[test]
    fn test_scoped_slice_sum() {
        let a = [1, 2, 3];
        let b = [10, 20, 30];
        let (sum_a, sum_b) = scoped_slice_sum(&a, &b);
        assert_eq!(sum_a, 6);
        assert_eq!(sum_b, 60);
        // Ensure slices are still accessible (they are borrowed, not moved).
        assert_eq!(a.len(), 3);
        assert_eq!(b.len(), 3);
    }

    #[test]
    fn test_handle_panic_ok() {
        let result = handle_panic(100, false);
        assert_eq!(result, Ok(100));
    }

    #[test]
    fn test_handle_panic_error() {
        let result = handle_panic(100, true);
        assert_eq!(result, Err(()));
    }
}
