/* A spinning read-write lock that keeps readers behind queued writers. */

use std::cell::UnsafeCell;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU32, Ordering};

const READER_MASK: u32 = (1 << 30) - 1;
const WRITER_HOLDING: u32 = 1 << 30;
const WRITER_WAITING: u32 = 1 << 31;

pub struct RwLock<T> {
    state: AtomicU32,
    // A waiting bit alone cannot preserve priority across multiple writers.
    writers: AtomicU32,
    data: UnsafeCell<T>,
}

unsafe impl<T: Send> Send for RwLock<T> {}
unsafe impl<T: Send + Sync> Sync for RwLock<T> {}

impl<T> RwLock<T> {
    pub const fn new(data: T) -> Self {
        Self {
            state: AtomicU32::new(0),
            writers: AtomicU32::new(0),
            data: UnsafeCell::new(data),
        }
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        loop {
            let state = self.state.load(Ordering::Acquire);
            if state & (WRITER_HOLDING | WRITER_WAITING) != 0
                || state & READER_MASK == READER_MASK
                || self.writers.load(Ordering::SeqCst) != 0
            {
                std::hint::spin_loop();
                continue;
            }
            if self
                .state
                .compare_exchange_weak(state, state + 1, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                // A writer may register between the first check and this CAS.
                if self.writers.load(Ordering::SeqCst) == 0 {
                    return RwLockReadGuard { lock: self };
                }
                self.state.fetch_sub(1, Ordering::Release);
            }
        }
    }

    pub fn write(&self) -> RwLockWriteGuard<'_, T> {
        self.writers.fetch_add(1, Ordering::SeqCst);
        self.state.fetch_or(WRITER_WAITING, Ordering::Release);
        loop {
            let state = self.state.load(Ordering::Acquire);
            if state & (READER_MASK | WRITER_HOLDING) == 0
                && self
                    .state
                    .compare_exchange_weak(
                        state,
                        state | WRITER_HOLDING,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    )
                    .is_ok()
            {
                return RwLockWriteGuard { lock: self };
            }
            std::hint::spin_loop();
        }
    }
}

pub struct RwLockReadGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> Deref for RwLockReadGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        // Readers cannot coexist with a writer.
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> Drop for RwLockReadGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.state.fetch_sub(1, Ordering::Release);
    }
}

pub struct RwLockWriteGuard<'a, T> {
    lock: &'a RwLock<T>,
}

impl<T> Deref for RwLockWriteGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for RwLockWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // WRITER_HOLDING excludes all other data access.
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for RwLockWriteGuard<'_, T> {
    fn drop(&mut self) {
        self.lock
            .state
            .fetch_and(!WRITER_HOLDING, Ordering::Release);
        if self.lock.writers.fetch_sub(1, Ordering::SeqCst) == 1 {
            self.lock
                .state
                .fetch_and(!WRITER_WAITING, Ordering::Release);
        }
    }
}

#[cfg(test)]
mod edge_tests {
    use super::*;
    use std::sync::{mpsc, Arc};

    #[test]
    fn queued_writers_run_before_a_late_reader() {
        let lock = Arc::new(RwLock::new(Vec::new()));
        let initial_reader = lock.read();
        let (events_tx, events_rx) = mpsc::channel();
        let writers: Vec<_> = (0..2)
            .map(|id| {
                let lock = lock.clone();
                let events = events_tx.clone();
                std::thread::spawn(move || {
                    let mut guard = lock.write();
                    guard.push(id);
                    events.send("writer").unwrap();
                })
            })
            .collect();
        // Both writers are registered but cannot pass the initial reader.
        while lock.writers.load(Ordering::SeqCst) != 2 {
            std::thread::yield_now();
        }
        let (ready_tx, ready_rx) = mpsc::channel();
        let reader_lock = lock.clone();
        let reader = std::thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let guard = reader_lock.read();
            assert_eq!(guard.len(), 2);
            events_tx.send("reader").unwrap();
        });
        ready_rx.recv().unwrap();
        drop(initial_reader);
        let events: Vec<_> = (0..3).map(|_| events_rx.recv().unwrap()).collect();
        assert_eq!(events, ["writer", "writer", "reader"]);
        for writer in writers {
            writer.join().unwrap();
        }
        reader.join().unwrap();
        assert_eq!(lock.writers.load(Ordering::SeqCst), 0);
        assert_eq!(lock.state.load(Ordering::Acquire), 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_multiple_readers() {
        let lock = Arc::new(RwLock::new(0u32));
        let mut handles = vec![];
        for _ in 0..10 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                let g = l.read();
                assert_eq!(*g, 0);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_writer_excludes_readers() {
        let lock = Arc::new(RwLock::new(0u32));
        let lock_w = Arc::clone(&lock);
        let writer = thread::spawn(move || {
            let mut g = lock_w.write();
            *g = 42;
        });
        writer.join().unwrap();
        let g = lock.read();
        assert_eq!(*g, 42);
    }

    #[test]
    fn test_concurrent_reads_after_write() {
        let lock = Arc::new(RwLock::new(Vec::<i32>::new()));
        {
            let mut g = lock.write();
            g.push(1);
            g.push(2);
        }
        let mut handles = vec![];
        for _ in 0..5 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                let g = l.read();
                assert_eq!(g.len(), 2);
                assert_eq!(&*g, &[1, 2]);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }

    #[test]
    fn test_concurrent_writes_serialized() {
        let lock = Arc::new(RwLock::new(0u64));
        let mut handles = vec![];
        for _ in 0..10 {
            let l = Arc::clone(&lock);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let mut g = l.write();
                    *g += 1;
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*lock.read(), 1000);
    }
}
