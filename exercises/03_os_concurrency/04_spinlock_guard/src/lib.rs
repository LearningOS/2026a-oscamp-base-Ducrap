/* Release a spin lock automatically when its exclusive guard is dropped. */

use std::cell::UnsafeCell;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicBool, Ordering};

pub struct SpinLock<T> {
    locked: AtomicBool,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Sync for SpinLock<T> {}
unsafe impl<T: Send> Send for SpinLock<T> {}

pub struct SpinGuard<'a, T> {
    lock: &'a SpinLock<T>,
    // Sharing a guard exposes &T, so Sync must additionally require T: Sync.
    marker: PhantomData<&'a mut T>,
}

impl<T> SpinLock<T> {
    pub fn new(data: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> SpinGuard<'_, T> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SpinGuard {
            lock: self,
            marker: PhantomData,
        }
    }
}

impl<T> Deref for SpinGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // The guard owns the lock, and shared borrows cannot overlap mutable ones.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for SpinGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for SpinGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;

    #[test]
    fn unwind_releases_the_guard() {
        let lock = SpinLock::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            *lock.lock() = 41;
            let _guard = lock.lock();
            panic!("unwind while holding the lock");
        }));
        assert!(result.is_err());
        assert_eq!(*lock.lock(), 41);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_guard_auto_release() {
        let lock = SpinLock::new(0u32);
        {
            let mut guard = lock.lock();
            *guard = 42;
            // guard drops here, automatically releasing lock
        }
        // Should be able to acquire lock again
        let guard = lock.lock();
        assert_eq!(*guard, 42);
    }

    #[test]
    fn test_guard_deref() {
        let lock = SpinLock::new(String::from("hello"));
        let guard = lock.lock();
        assert_eq!(guard.len(), 5);
        assert_eq!(&*guard, "hello");
    }

    #[test]
    fn test_guard_deref_mut() {
        let lock = SpinLock::new(Vec::<i32>::new());
        {
            let mut guard = lock.lock();
            guard.push(1);
            guard.push(2);
            guard.push(3);
        }
        let guard = lock.lock();
        assert_eq!(&*guard, &[1, 2, 3]);
    }

    #[test]
    fn test_concurrent_with_guard() {
        let lock = Arc::new(SpinLock::new(0u64));
        let mut handles = vec![];

        for _ in 0..10 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                for _ in 0..1000 {
                    let mut guard = l.lock();
                    *guard += 1;
                    // guard automatically released
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(*lock.lock(), 10000);
    }

    #[test]
    fn test_panic_safety() {
        let lock = Arc::new(SpinLock::new(0u32));
        let l = Arc::clone(&lock);

        let result = thread::spawn(move || {
            let mut guard = l.lock();
            *guard = 42;
            panic!("intentional panic");
        })
        .join();

        assert!(result.is_err());
        // Even if thread panics, guard's Drop should release lock
        // Note: this test may have different results due to panic unwind behavior
    }
}
