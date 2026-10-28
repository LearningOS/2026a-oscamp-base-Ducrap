/*
 * A no_std bump allocator with checked alignment and atomic reservations.
 * Individual deallocations do not reclaim storage.
 */

#![cfg_attr(not(test), no_std)]

use core::alloc::{GlobalAlloc, Layout};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicUsize, Ordering};

pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next: AtomicUsize,
}

impl BumpAllocator {
    /// Create a new BumpAllocator.
    ///
    /// # Safety
    /// `heap_start..heap_end` must be a valid, readable and writable memory region,
    /// and must not be used by other code during this allocator's lifetime.
    pub const unsafe fn new(heap_start: usize, heap_end: usize) -> Self {
        Self {
            heap_start,
            heap_end,
            next: AtomicUsize::new(heap_start),
        }
    }

    /// Reset the allocator (free all allocated memory).
    ///
    /// Call only after all earlier allocations have stopped being used and
    /// while no other thread is allocating or resetting this allocator.
    pub fn reset(&self) {
        self.next.store(self.heap_start, Ordering::SeqCst);
    }
}

// SAFETY: Checked bounds and CAS reserve disjoint regions; the constructor and
// reset contracts require exclusive backing storage and quiescent resets.
unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut next = self.next.load(Ordering::SeqCst);
        loop {
            let Some(aligned) = next.checked_add(layout.align() - 1) else {
                return null_mut();
            };
            let aligned = aligned & !(layout.align() - 1);
            let Some(end) = aligned.checked_add(layout.size()) else {
                return null_mut();
            };
            if aligned < self.heap_start || end > self.heap_end {
                return null_mut();
            }
            match self
                .next
                .compare_exchange(next, end, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return aligned as *mut u8,
                Err(current) => next = current,
            }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator does not reclaim individual objects — leave empty
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    const HEAP_SIZE: usize = 4096;

    fn make_allocator() -> (BumpAllocator, Vec<u8>) {
        let mut heap = vec![0u8; HEAP_SIZE];
        let start = heap.as_mut_ptr() as usize;
        let alloc = unsafe { BumpAllocator::new(start, start + HEAP_SIZE) };
        (alloc, heap)
    }

    #[test]
    fn test_alloc_basic() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(16, 8).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(!ptr.is_null(), "allocation should succeed");
    }

    #[test]
    fn test_alloc_alignment() {
        let (alloc, _heap) = make_allocator();
        for align in [1, 2, 4, 8, 16, 64] {
            let layout = Layout::from_size_align(1, align).unwrap();
            let ptr = unsafe { alloc.alloc(layout) };
            assert!(!ptr.is_null());
            assert_eq!(
                ptr as usize % align,
                0,
                "returned address must satisfy align={align}"
            );
        }
    }

    #[test]
    fn test_alloc_no_overlap() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(64, 8).unwrap();
        let p1 = unsafe { alloc.alloc(layout) } as usize;
        let p2 = unsafe { alloc.alloc(layout) } as usize;
        assert!(
            p1 + 64 <= p2 || p2 + 64 <= p1,
            "two allocations must not overlap"
        );
    }

    #[test]
    fn test_alloc_oom() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(HEAP_SIZE + 1, 1).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(ptr.is_null(), "should return null when exceeding heap");
    }

    #[test]
    fn test_alloc_fill_heap() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(256, 1).unwrap();
        for i in 0..16 {
            let ptr = unsafe { alloc.alloc(layout) };
            assert!(!ptr.is_null(), "allocation #{i} should succeed");
        }
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(ptr.is_null(), "should return null when heap is full");
    }

    #[test]
    fn test_reset() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(HEAP_SIZE, 1).unwrap();
        let p1 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null());
        alloc.reset();
        let p2 = unsafe { alloc.alloc(layout) };
        assert!(!p2.is_null(), "should be able to allocate after reset");
        assert_eq!(
            p1, p2,
            "address after reset should match the first allocation"
        );
    }

    #[test]
    fn test_failed_allocation_preserves_remaining_space() {
        let (alloc, _heap) = make_allocator();
        // SAFETY: The test keeps the backing heap alive and uses nonzero layouts.
        unsafe {
            let first = alloc.alloc(Layout::from_size_align(64, 1).unwrap());
            assert!(!first.is_null());
            assert!(alloc
                .alloc(Layout::from_size_align(HEAP_SIZE, 1).unwrap())
                .is_null());
            let rest = alloc.alloc(Layout::from_size_align(HEAP_SIZE - 64, 1).unwrap());
            assert_eq!(rest, first.add(64));
        }
    }

    #[test]
    fn test_concurrent_allocations_do_not_overlap() {
        let (alloc, _heap) = make_allocator();
        let mut addresses = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for _ in 0..8 {
                let allocator = &alloc;
                handles.push(scope.spawn(move || {
                    (0..16)
                        .map(|_| {
                            // SAFETY: The backing heap outlives all scoped workers.
                            let ptr =
                                unsafe { allocator.alloc(Layout::from_size_align(16, 8).unwrap()) };
                            assert!(!ptr.is_null());
                            ptr as usize
                        })
                        .collect::<Vec<_>>()
                }));
            }
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        addresses.sort_unstable();
        assert_eq!(addresses.len(), 128);
        assert!(addresses.windows(2).all(|pair| pair[0] + 16 <= pair[1]));
    }
}
