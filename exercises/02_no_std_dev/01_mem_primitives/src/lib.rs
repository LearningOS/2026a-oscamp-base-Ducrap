/*
 * Byte-wise memory and C-string operations for no_std environments.
 * Copy direction preserves overlapping regions in memmove.
 */

// Force no_std in production; allow std in tests (cargo test framework requires it)
#![cfg_attr(not(test), no_std)]

/// Copy `n` bytes from `src` to `dst`.
///
/// - `dst` and `src` must not overlap (use `my_memmove` for overlapping regions)
/// - Returns `dst`
///
/// # Safety
/// For nonzero `n`, `src` must be readable and `dst` writable for `n` bytes.
/// The regions must not overlap. Null pointers are permitted when `n` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_memcpy(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    for offset in 0..n {
        // SAFETY: The caller supplies disjoint valid regions covering this byte.
        unsafe { dst.add(offset).write(src.add(offset).read()) };
    }
    dst
}

/// Set `n` bytes starting at `dst` to the value `c`.
///
/// Returns `dst`.
///
/// # Safety
/// For nonzero `n`, `dst` must be writable for `n` bytes.
/// A null pointer is permitted when `n` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_memset(dst: *mut u8, c: u8, n: usize) -> *mut u8 {
    for offset in 0..n {
        // SAFETY: Each destination byte is within the writable region.
        unsafe { dst.add(offset).write(c) };
    }
    dst
}

/// Copy `n` bytes from `src` to `dst`, correctly handling overlapping memory.
///
/// Returns `dst`.
///
/// # Safety
/// For nonzero `n`, `src` must be readable and `dst` writable for `n` bytes.
/// The regions may overlap. Null pointers are permitted when `n` is zero.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_memmove(dst: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    let dst_address = dst as usize;
    let src_address = src as usize;
    // Subtracting the lower address avoids overflowing an end-address sum.
    if dst_address > src_address && dst_address - src_address < n {
        for offset in (0..n).rev() {
            // SAFETY: Backward copying reads each source byte before overwriting it.
            unsafe { dst.add(offset).write(src.add(offset).read()) };
        }
    } else {
        for offset in 0..n {
            // SAFETY: The regions are valid and forward copying preserves unread bytes.
            unsafe { dst.add(offset).write(src.add(offset).read()) };
        }
    }
    dst
}

/// Return the length of a null-terminated byte string, excluding the trailing null.
///
/// # Safety
/// `s` must point to a valid null-terminated byte string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_strlen(s: *const u8) -> usize {
    let mut len = 0;
    // SAFETY: The caller guarantees a readable string with a terminating zero.
    while unsafe { s.add(len).read() } != 0 {
        len += 1;
    }
    len
}

/// Compare two null-terminated byte strings.
///
/// Returns:
/// - `0`  : strings are equal
/// - `< 0`: `s1` is lexicographically less than `s2`
/// - `> 0`: `s1` is lexicographically greater than `s2`
///
/// # Safety
/// `s1` and `s2` must each point to a valid null-terminated byte string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn my_strcmp(s1: *const u8, s2: *const u8) -> i32 {
    let mut offset = 0;
    loop {
        // SAFETY: Both strings remain readable up to and including their terminators.
        let (left, right) = unsafe { (s1.add(offset).read(), s2.add(offset).read()) };
        if left != right || left == 0 {
            return i32::from(left) - i32::from(right);
        }
        offset += 1;
    }
}

// ============================================================
// Tests (std is available under #[cfg(test)])
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memcpy_basic() {
        let src = [1u8, 2, 3, 4, 5];
        let mut dst = [0u8; 5];
        unsafe { my_memcpy(dst.as_mut_ptr(), src.as_ptr(), 5) };
        assert_eq!(dst, src);
    }

    #[test]
    fn test_memcpy_zero_len() {
        let src = [0xFFu8; 4];
        let mut dst = [0u8; 4];
        unsafe { my_memcpy(dst.as_mut_ptr(), src.as_ptr(), 0) };
        assert_eq!(dst, [0u8; 4]);
    }

    #[test]
    fn test_memset_basic() {
        let mut buf = [0u8; 8];
        unsafe { my_memset(buf.as_mut_ptr(), 0xAB, 8) };
        assert!(buf.iter().all(|&b| b == 0xAB));
    }

    #[test]
    fn test_memset_partial() {
        let mut buf = [0u8; 8];
        unsafe { my_memset(buf.as_mut_ptr(), 0xFF, 4) };
        assert_eq!(&buf[..4], &[0xFF; 4]);
        assert_eq!(&buf[4..], &[0x00; 4]);
    }

    #[test]
    fn test_memmove_no_overlap() {
        let src = [1u8, 2, 3, 4];
        let mut dst = [0u8; 4];
        unsafe { my_memmove(dst.as_mut_ptr(), src.as_ptr(), 4) };
        assert_eq!(dst, src);
    }

    #[test]
    fn test_memmove_overlap_forward() {
        // Copy buf[0..4] to buf[1..5], shifting right by 1
        let mut buf = [1u8, 2, 3, 4, 5];
        unsafe { my_memmove(buf.as_mut_ptr().add(1), buf.as_ptr(), 4) };
        assert_eq!(buf, [1, 1, 2, 3, 4]);
    }

    #[test]
    fn test_strlen_basic() {
        let s = b"hello\0";
        assert_eq!(unsafe { my_strlen(s.as_ptr()) }, 5);
    }

    #[test]
    fn test_strlen_empty() {
        let s = b"\0";
        assert_eq!(unsafe { my_strlen(s.as_ptr()) }, 0);
    }

    #[test]
    fn test_strcmp_equal() {
        let a = b"hello\0";
        let b = b"hello\0";
        assert_eq!(unsafe { my_strcmp(a.as_ptr(), b.as_ptr()) }, 0);
    }

    #[test]
    fn test_strcmp_less() {
        let a = b"abc\0";
        let b = b"abd\0";
        assert!(unsafe { my_strcmp(a.as_ptr(), b.as_ptr()) } < 0);
    }

    #[test]
    fn test_strcmp_greater() {
        let a = b"abd\0";
        let b = b"abc\0";
        assert!(unsafe { my_strcmp(a.as_ptr(), b.as_ptr()) } > 0);
    }

    #[test]
    fn test_zero_length_null_pointers() {
        // SAFETY: Zero-length operations must not dereference either pointer.
        unsafe {
            assert!(my_memcpy(core::ptr::null_mut(), core::ptr::null(), 0).is_null());
            assert!(my_memset(core::ptr::null_mut(), 0xFF, 0).is_null());
            assert!(my_memmove(core::ptr::null_mut(), core::ptr::null(), 0).is_null());
        }
    }

    #[test]
    fn test_memmove_left_and_in_place() {
        let mut buf = [1u8, 2, 3, 4, 5];
        let ptr = buf.as_mut_ptr();
        // SAFETY: Both ranges lie in buf; memmove permits this overlap.
        unsafe {
            assert_eq!(my_memmove(ptr, ptr.add(1), 4), ptr);
            assert_eq!(my_memmove(ptr, ptr, 5), ptr);
        }
        assert_eq!(buf, [2, 3, 4, 5, 5]);
    }

    #[test]
    fn test_strcmp_unsigned_and_prefix() {
        // SAFETY: All operands are initialized, null-terminated byte strings.
        unsafe {
            assert!(my_strcmp(b"\x80\0".as_ptr(), b"\x7F\0".as_ptr()) > 0);
            assert!(my_strcmp(b"ab\0".as_ptr(), b"abc\0".as_ptr()) < 0);
            assert!(my_strcmp(b"abc\0".as_ptr(), b"ab\0".as_ptr()) > 0);
        }
    }
}
