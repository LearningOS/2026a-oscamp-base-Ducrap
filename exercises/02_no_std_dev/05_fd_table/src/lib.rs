/*
 * A sparse file descriptor table with shared file ownership.
 * New descriptors reuse the smallest vacant slot.
 */

use std::sync::Arc;

/// File abstraction trait — all "files" in the kernel (regular files, pipes, sockets) implement this
pub trait File: Send + Sync {
    fn read(&self, buf: &mut [u8]) -> isize;
    fn write(&self, buf: &[u8]) -> isize;
}

/// File descriptor table
pub struct FdTable {
    entries: Vec<Option<Arc<dyn File>>>,
}

impl FdTable {
    /// Create an empty fd table
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Allocate a new fd, return the fd number.
    ///
    /// Prefers reusing the smallest closed fd number; if no free slot, appends to the end.
    pub fn alloc(&mut self, file: Arc<dyn File>) -> usize {
        for (fd, entry) in self.entries.iter_mut().enumerate() {
            if entry.is_none() {
                *entry = Some(file);
                return fd;
            }
        }
        let fd = self.entries.len();
        self.entries.push(Some(file));
        fd
    }

    /// Get the file object for an fd. Returns None if the fd doesn't exist or is closed.
    pub fn get(&self, fd: usize) -> Option<Arc<dyn File>> {
        self.entries.get(fd).cloned().flatten()
    }

    /// Close an fd. Returns true on success, false if the fd doesn't exist or is already closed.
    pub fn close(&mut self, fd: usize) -> bool {
        self.entries.get_mut(fd).and_then(Option::take).is_some()
    }

    /// Return the number of currently allocated fds (excluding closed ones)
    pub fn count(&self) -> usize {
        self.entries.iter().filter(|entry| entry.is_some()).count()
    }
}

impl Default for FdTable {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// Test File implementation
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct MockFile {
        id: usize,
        write_log: Mutex<Vec<Vec<u8>>>,
    }

    impl MockFile {
        fn new(id: usize) -> Arc<Self> {
            Arc::new(Self {
                id,
                write_log: Mutex::new(vec![]),
            })
        }
    }

    impl File for MockFile {
        fn read(&self, buf: &mut [u8]) -> isize {
            buf[0] = self.id as u8;
            1
        }
        fn write(&self, buf: &[u8]) -> isize {
            self.write_log.lock().unwrap().push(buf.to_vec());
            buf.len() as isize
        }
    }

    #[test]
    fn test_alloc_basic() {
        let mut table = FdTable::new();
        let fd = table.alloc(MockFile::new(0));
        assert_eq!(fd, 0, "first fd should be 0");
        let fd2 = table.alloc(MockFile::new(1));
        assert_eq!(fd2, 1, "second fd should be 1");
    }

    #[test]
    fn test_get() {
        let mut table = FdTable::new();
        let file = MockFile::new(42);
        let fd = table.alloc(file);
        let got = table.get(fd);
        assert!(got.is_some(), "get should return Some");
        let mut buf = [0u8; 1];
        got.unwrap().read(&mut buf);
        assert_eq!(buf[0], 42);
    }

    #[test]
    fn test_get_invalid() {
        let table = FdTable::new();
        assert!(table.get(0).is_none());
        assert!(table.get(999).is_none());
    }

    #[test]
    fn test_close_and_reuse() {
        let mut table = FdTable::new();
        let fd0 = table.alloc(MockFile::new(0)); // fd=0
        let fd1 = table.alloc(MockFile::new(1)); // fd=1
        let fd2 = table.alloc(MockFile::new(2)); // fd=2

        assert!(table.close(fd1), "closing fd=1 should succeed");
        assert!(
            table.get(fd1).is_none(),
            "get should return None after close"
        );

        // Next allocation should reuse fd=1 (smallest free)
        let fd_new = table.alloc(MockFile::new(99));
        assert_eq!(fd_new, fd1, "should reuse the smallest closed fd");

        let _ = (fd0, fd2);
    }

    #[test]
    fn test_close_invalid() {
        let mut table = FdTable::new();
        assert!(
            !table.close(0),
            "closing non-existent fd should return false"
        );
    }

    #[test]
    fn test_count() {
        let mut table = FdTable::new();
        assert_eq!(table.count(), 0);
        let fd0 = table.alloc(MockFile::new(0));
        let fd1 = table.alloc(MockFile::new(1));
        assert_eq!(table.count(), 2);
        table.close(fd0);
        assert_eq!(table.count(), 1);
        table.close(fd1);
        assert_eq!(table.count(), 0);
    }

    #[test]
    fn test_write_through_fd() {
        let mut table = FdTable::new();
        let file = MockFile::new(0);
        let fd = table.alloc(file);
        let f = table.get(fd).unwrap();
        let n = f.write(b"hello");
        assert_eq!(n, 5);
    }

    #[test]
    fn test_reuses_smallest_of_multiple_holes() {
        let mut table = FdTable::new();
        let fds: Vec<_> = (0..4).map(|id| table.alloc(MockFile::new(id))).collect();
        assert!(table.close(fds[3]));
        assert!(table.close(fds[1]));
        assert_eq!(table.alloc(MockFile::new(10)), 1);
        assert_eq!(table.alloc(MockFile::new(11)), 3);
        assert_eq!(table.count(), 4);
    }

    #[test]
    fn test_close_releases_only_table_reference() {
        let mut table = FdTable::new();
        let file = MockFile::new(42);
        let fd = table.alloc(file.clone());
        let retained = table.get(fd).unwrap();
        assert_eq!(Arc::strong_count(&file), 3);
        assert!(table.close(fd));
        assert_eq!(Arc::strong_count(&file), 2);
        assert!(!table.close(fd));
        let mut buf = [0];
        assert_eq!(retained.read(&mut buf), 1);
        assert_eq!(buf, [42]);
        drop(retained);
        assert_eq!(Arc::strong_count(&file), 1);
        assert!(table.get(usize::MAX).is_none());
        assert!(!table.close(usize::MAX));
    }
}
