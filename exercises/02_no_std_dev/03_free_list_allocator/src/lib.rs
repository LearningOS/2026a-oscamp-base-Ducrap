/*
 * A first-fit free-list allocator with atomic bump fallback.
 * A spin lock protects complete list operations; allocation is not reentrant.
 */

#![cfg_attr(not(test), no_std)]

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::ptr::null_mut;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Free block header, stored at the beginning of each free memory block
struct FreeBlock {
    size: usize,
    next: *mut FreeBlock,
}

pub struct FreeListAllocator {
    heap_start: usize,
    heap_end: usize,
    /// Bump pointer: unallocated region starts here
    bump_next: AtomicUsize,
    free_list: UnsafeCell<*mut FreeBlock>,
    list_locked: AtomicBool,
}

// SAFETY: Heap addresses are owned by the allocator and all list access is locked.
unsafe impl Send for FreeListAllocator {}
// SAFETY: The spin lock protects free headers; the bump cursor is atomic.
unsafe impl Sync for FreeListAllocator {}

struct ListGuard<'a>(&'a AtomicBool);

impl Drop for ListGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

impl FreeListAllocator {
    /// # Safety
    /// `heap_start..heap_end` must remain valid, readable, writable, and exclusive
    /// to this allocator for as long as the allocator or its allocations are used.
    pub unsafe fn new(heap_start: usize, heap_end: usize) -> Self {
        Self {
            heap_start,
            heap_end,
            bump_next: AtomicUsize::new(heap_start),
            free_list: UnsafeCell::new(null_mut()),
            list_locked: AtomicBool::new(false),
        }
    }

    fn with_free_list<T>(&self, operation: impl FnOnce(&mut *mut FreeBlock) -> T) -> T {
        while self
            .list_locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let _guard = ListGuard(&self.list_locked);
        // SAFETY: Only the thread holding this lock accesses the head or free headers.
        operation(unsafe { &mut *self.free_list.get() })
    }
}

// SAFETY: Atomic reservations do not overlap, and free headers are accessed
// only while the list lock is held and the corresponding block is not in use.
unsafe impl GlobalAlloc for FreeListAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // Ensure block is at least large enough to hold a FreeBlock header (for future dealloc)
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());
        let align = layout.align().max(core::mem::align_of::<FreeBlock>());

        let reused = self.with_free_list(|head| {
            let mut link: *mut *mut FreeBlock = head;
            // SAFETY: Each link points to the locked head or an initialized free header.
            unsafe {
                while !(*link).is_null() {
                    let block = *link;
                    if (block as usize) % align == 0 && (*block).size >= size {
                        *link = (*block).next;
                        return Some(block.cast::<u8>());
                    }
                    link = core::ptr::addr_of_mut!((*block).next);
                }
            }
            None
        });
        if let Some(block) = reused {
            return block;
        }

        let mut next = self.bump_next.load(Ordering::SeqCst);
        loop {
            let Some(aligned) = next.checked_add(align - 1) else {
                return null_mut();
            };
            let aligned = aligned & !(align - 1);
            let Some(end) = aligned.checked_add(size) else {
                return null_mut();
            };
            if aligned < self.heap_start || end > self.heap_end {
                return null_mut();
            }
            match self
                .bump_next
                .compare_exchange(next, end, Ordering::SeqCst, Ordering::SeqCst)
            {
                Ok(_) => return aligned as *mut u8,
                Err(current) => next = current,
            }
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let size = layout.size().max(core::mem::size_of::<FreeBlock>());

        self.with_free_list(|head| {
            let block = ptr.cast::<FreeBlock>();
            // SAFETY: alloc reserves header size/alignment, and the caller has
            // relinquished this live allocation with its original layout.
            unsafe { block.write(FreeBlock { size, next: *head }) };
            *head = block;
        });
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    const HEAP_SIZE: usize = 4096;

    fn make_allocator() -> (FreeListAllocator, Vec<u8>) {
        let mut heap = vec![0u8; HEAP_SIZE];
        let start = heap.as_mut_ptr() as usize;
        let alloc = unsafe { FreeListAllocator::new(start, start + HEAP_SIZE) };
        (alloc, heap)
    }

    #[test]
    fn test_alloc_basic() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(32, 8).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(!ptr.is_null());
    }

    #[test]
    fn test_alloc_alignment() {
        let (alloc, _heap) = make_allocator();
        for align in [1, 2, 4, 8, 16] {
            let layout = Layout::from_size_align(8, align).unwrap();
            let ptr = unsafe { alloc.alloc(layout) };
            assert!(!ptr.is_null());
            assert_eq!(ptr as usize % align, 0, "align={align}");
        }
    }

    #[test]
    fn test_dealloc_and_reuse() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(64, 8).unwrap();

        let p1 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null());

        // After freeing, the next allocation should reuse the same block
        unsafe { alloc.dealloc(p1, layout) };
        let p2 = unsafe { alloc.alloc(layout) };
        assert!(!p2.is_null());
        assert_eq!(p1, p2, "should reuse the freed block");
    }

    #[test]
    fn test_multiple_alloc_dealloc() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(128, 8).unwrap();

        let p1 = unsafe { alloc.alloc(layout) };
        let p2 = unsafe { alloc.alloc(layout) };
        let p3 = unsafe { alloc.alloc(layout) };
        assert!(!p1.is_null() && !p2.is_null() && !p3.is_null());

        unsafe { alloc.dealloc(p2, layout) };
        unsafe { alloc.dealloc(p1, layout) };

        let q1 = unsafe { alloc.alloc(layout) };
        let q2 = unsafe { alloc.alloc(layout) };
        assert!(!q1.is_null() && !q2.is_null());
    }

    #[test]
    fn test_oom() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(HEAP_SIZE + 1, 1).unwrap();
        let ptr = unsafe { alloc.alloc(layout) };
        assert!(ptr.is_null(), "should return null when exceeding heap");
    }

    #[test]
    fn test_first_fit_skips_small_head() {
        let (alloc, _heap) = make_allocator();
        let small = Layout::from_size_align(32, 8).unwrap();
        let large = Layout::from_size_align(128, 8).unwrap();
        // SAFETY: Each live allocation is freed once with its original layout.
        unsafe {
            let large_ptr = alloc.alloc(large);
            let small_ptr = alloc.alloc(small);
            assert!(!large_ptr.is_null() && !small_ptr.is_null());
            alloc.dealloc(large_ptr, large);
            alloc.dealloc(small_ptr, small);
            assert_eq!(alloc.alloc(large), large_ptr);
            assert_eq!(alloc.alloc(small), small_ptr);
        }
    }

    #[test]
    fn test_small_allocations_reserve_free_headers() {
        let (alloc, _heap) = make_allocator();
        let layout = Layout::from_size_align(1, 1).unwrap();
        // SAFETY: The allocator must reserve room for a header even for one byte.
        unsafe {
            let first = alloc.alloc(layout);
            let second = alloc.alloc(layout);
            assert!(!first.is_null() && !second.is_null());
            second.write(0xAB);
            alloc.dealloc(first, layout);
            assert_eq!(second.read(), 0xAB);
            assert_eq!(alloc.alloc(layout), first);
        }
    }

    #[test]
    fn test_concurrent_reuse_preserves_live_blocks() {
        let (alloc, _heap) = make_allocator();
        std::thread::scope(|scope| {
            for worker in 0usize..8 {
                let allocator = &alloc;
                scope.spawn(move || {
                    let layout =
                        Layout::from_size_align(32, core::mem::align_of::<usize>()).unwrap();
                    for _ in 0..100 {
                        // SAFETY: The scoped heap stays alive. Each worker accesses
                        // only its own live block and frees it with the same layout.
                        unsafe {
                            let ptr = allocator.alloc(layout);
                            assert!(!ptr.is_null());
                            ptr.cast::<usize>().write(worker);
                            std::thread::yield_now();
                            assert_eq!(ptr.cast::<usize>().read(), worker);
                            allocator.dealloc(ptr, layout);
                        }
                    }
                });
            }
        });
    }
}
