/*
 * Linux syscall ABI descriptions and real three-argument syscall wrappers.
 * Register bindings and syscall numbers follow each supported architecture.
 */

#![cfg_attr(not(test), no_std)]

/// Describes a Linux Syscall ABI for a specific architecture
pub struct SyscallABI {
    /// Architecture name: "x86_64", "aarch64", "riscv64"
    pub arch: &'static str,
    /// Instruction that triggers the syscall: "syscall", "svc #0", "ecall"
    pub instruction: &'static str,
    /// Register holding the syscall number
    pub id_reg: &'static str,
    /// Register holding the return value
    pub ret_reg: &'static str,
    /// Argument registers (in order)
    pub arg_regs: &'static [&'static str],
    /// Registers additionally clobbered by the syscall instruction
    pub clobbered: &'static [&'static str],
    /// write syscall number
    pub sys_write: usize,
    /// read syscall number
    pub sys_read: usize,
    /// close syscall number
    pub sys_close: usize,
    /// exit syscall number
    pub sys_exit: usize,
}

/// Return the x86_64 Linux syscall ABI description
pub fn x86_64_abi() -> SyscallABI {
    SyscallABI {
        arch: "x86_64",
        instruction: "syscall",
        id_reg: "rax",
        ret_reg: "rax",
        arg_regs: &["rdi", "rsi", "rdx", "r10", "r8", "r9"],
        clobbered: &["rcx", "r11"],
        sys_write: 1,
        sys_read: 0,
        sys_close: 3,
        sys_exit: 60,
    }
}

/// Return the aarch64 Linux syscall ABI description
pub fn aarch64_abi() -> SyscallABI {
    SyscallABI {
        arch: "aarch64",
        instruction: "svc #0",
        id_reg: "x8",
        ret_reg: "x0",
        arg_regs: &["x0", "x1", "x2", "x3", "x4", "x5"],
        clobbered: &[],
        sys_write: 64,
        sys_read: 63,
        sys_close: 57,
        sys_exit: 93,
    }
}

/// Return the riscv64 Linux syscall ABI description
pub fn riscv64_abi() -> SyscallABI {
    SyscallABI {
        arch: "riscv64",
        instruction: "ecall",
        id_reg: "a7",
        ret_reg: "a0",
        arg_regs: &["a0", "a1", "a2", "a3", "a4", "a5"],
        clobbered: &[],
        sys_write: 64,
        sys_read: 63,
        sys_close: 57,
        sys_exit: 93,
    }
}

// ============================================================
// Real syscall implementation (conditional compilation, only active on matching platform)
// ============================================================

/// Issue a Linux syscall with up to 3 arguments.
///
/// # Safety
/// The caller must ensure the syscall number and arguments are valid.
#[cfg(all(target_arch = "x86_64", target_os = "linux"))]
pub unsafe fn syscall3(id: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    let ret: isize;
    // SAFETY: The caller validates the syscall and its arguments. Memory effects
    // remain enabled, and syscall's two extra register clobbers are declared.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") id => ret,
            in("rdi") arg0,
            in("rsi") arg1,
            in("rdx") arg2,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    ret
}

/// Issue an aarch64 Linux syscall with up to three arguments.
///
/// # Safety
/// The caller must validate the syscall number and all arguments.
#[cfg(all(target_arch = "aarch64", target_os = "linux"))]
pub unsafe fn syscall3(id: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    let ret: isize;
    // SAFETY: The caller validates the syscall and arguments; x0 carries its result.
    unsafe {
        core::arch::asm!(
            "svc #0",
            in("x8") id,
            inlateout("x0") arg0 => ret,
            in("x1") arg1,
            in("x2") arg2,
            options(nostack),
        );
    }
    ret
}

/// Issue a riscv64 Linux syscall with up to three arguments.
///
/// # Safety
/// The caller must validate the syscall number and all arguments.
#[cfg(all(target_arch = "riscv64", target_os = "linux"))]
pub unsafe fn syscall3(id: usize, arg0: usize, arg1: usize, arg2: usize) -> isize {
    let ret: isize;
    // SAFETY: The caller validates the syscall and arguments; a0 carries its result.
    unsafe {
        core::arch::asm!(
            "ecall",
            in("a7") id,
            inlateout("a0") arg0 => ret,
            in("a1") arg1,
            in("a2") arg2,
            options(nostack),
        );
    }
    ret
}

/// Unsupported targets cannot invoke this syscall ABI.
///
/// # Safety
/// The caller must validate the syscall number and all arguments.
#[cfg(not(all(
    target_os = "linux",
    any(
        target_arch = "x86_64",
        target_arch = "aarch64",
        target_arch = "riscv64"
    )
)))]
pub unsafe fn syscall3(_id: usize, _arg0: usize, _arg1: usize, _arg2: usize) -> isize {
    panic!("syscall3 requires Linux on x86_64, aarch64, or riscv64")
}

// Platform-specific write syscall number
#[cfg(target_arch = "x86_64")]
const NATIVE_SYS_WRITE: usize = 1;
#[cfg(target_arch = "x86_64")]
const NATIVE_SYS_READ: usize = 0;
#[cfg(target_arch = "x86_64")]
const NATIVE_SYS_CLOSE: usize = 3;
#[cfg(target_arch = "x86_64")]
const NATIVE_SYS_EXIT: usize = 60;

#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const NATIVE_SYS_WRITE: usize = 64;
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const NATIVE_SYS_READ: usize = 63;
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const NATIVE_SYS_CLOSE: usize = 57;
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const NATIVE_SYS_EXIT: usize = 93;

// Fallback for other architectures (not actually used, just for compilation)
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
const NATIVE_SYS_WRITE: usize = 0;
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
const NATIVE_SYS_READ: usize = 0;
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
const NATIVE_SYS_CLOSE: usize = 0;
#[cfg(not(any(
    target_arch = "x86_64",
    target_arch = "aarch64",
    target_arch = "riscv64"
)))]
const NATIVE_SYS_EXIT: usize = 0;

/// Write data from `buf` to file descriptor `fd`.
pub fn sys_write(fd: usize, buf: &[u8]) -> isize {
    // SAFETY: The shared slice remains readable for the duration of this syscall.
    unsafe { syscall3(NATIVE_SYS_WRITE, fd, buf.as_ptr() as usize, buf.len()) }
}

/// Read data from file descriptor `fd` into `buf`.
pub fn sys_read(fd: usize, buf: &mut [u8]) -> isize {
    // SAFETY: The exclusive slice remains writable for the duration of this syscall.
    unsafe { syscall3(NATIVE_SYS_READ, fd, buf.as_mut_ptr() as usize, buf.len()) }
}

/// Close file descriptor `fd`.
pub fn sys_close(fd: usize) -> isize {
    // SAFETY: close has no pointer arguments; an invalid descriptor returns an error.
    unsafe { syscall3(NATIVE_SYS_CLOSE, fd, 0, 0) }
}

/// Exit the calling Linux thread, or the process if it is the last thread.
pub fn sys_exit(code: i32) -> ! {
    // SAFETY: exit takes only an integer status and ordinarily never returns.
    unsafe { syscall3(NATIVE_SYS_EXIT, code as usize, 0, 0) };
    // A sandbox can deny exit; preserve the diverging contract without assuming
    // that returning from the syscall would make unreachable code sound.
    loop {
        core::hint::spin_loop();
    }
}

// ============================================================
// Tests
// ============================================================
#[cfg(test)]
mod tests {
    use super::*;

    // ---- ABI knowledge tests (run on any platform) ----

    #[test]
    fn test_x86_64_instruction() {
        let abi = x86_64_abi();
        assert_eq!(abi.arch, "x86_64");
        assert_eq!(abi.instruction, "syscall");
    }

    #[test]
    fn test_x86_64_registers() {
        let abi = x86_64_abi();
        assert_eq!(abi.id_reg, "rax");
        assert_eq!(abi.ret_reg, "rax");
        assert_eq!(
            abi.arg_regs,
            &["rdi", "rsi", "rdx", "r10", "r8", "r9"],
            "x86_64 argument register order is incorrect"
        );
    }

    #[test]
    fn test_x86_64_clobbered() {
        let abi = x86_64_abi();
        assert!(
            abi.clobbered.contains(&"rcx") && abi.clobbered.contains(&"r11"),
            "x86_64 syscall clobbers rcx and r11"
        );
    }

    #[test]
    fn test_x86_64_syscall_numbers() {
        let abi = x86_64_abi();
        assert_eq!(abi.sys_write, 1);
        assert_eq!(abi.sys_read, 0);
        assert_eq!(abi.sys_close, 3);
        assert_eq!(abi.sys_exit, 60);
    }

    #[test]
    fn test_aarch64_instruction() {
        let abi = aarch64_abi();
        assert_eq!(abi.arch, "aarch64");
        assert_eq!(abi.instruction, "svc #0");
    }

    #[test]
    fn test_aarch64_registers() {
        let abi = aarch64_abi();
        assert_eq!(abi.id_reg, "x8");
        assert_eq!(abi.ret_reg, "x0");
        assert_eq!(
            abi.arg_regs,
            &["x0", "x1", "x2", "x3", "x4", "x5"],
            "aarch64 argument register order is incorrect"
        );
    }

    #[test]
    fn test_aarch64_clobbered() {
        let abi = aarch64_abi();
        assert!(
            abi.clobbered.is_empty(),
            "aarch64 svc does not clobber additional registers"
        );
    }

    #[test]
    fn test_aarch64_syscall_numbers() {
        let abi = aarch64_abi();
        assert_eq!(abi.sys_write, 64);
        assert_eq!(abi.sys_read, 63);
        assert_eq!(abi.sys_close, 57);
        assert_eq!(abi.sys_exit, 93);
    }

    #[test]
    fn test_riscv64_instruction() {
        let abi = riscv64_abi();
        assert_eq!(abi.arch, "riscv64");
        assert_eq!(abi.instruction, "ecall");
    }

    #[test]
    fn test_riscv64_registers() {
        let abi = riscv64_abi();
        assert_eq!(abi.id_reg, "a7");
        assert_eq!(abi.ret_reg, "a0");
        assert_eq!(
            abi.arg_regs,
            &["a0", "a1", "a2", "a3", "a4", "a5"],
            "riscv64 argument register order is incorrect"
        );
    }

    #[test]
    fn test_riscv64_clobbered() {
        let abi = riscv64_abi();
        assert!(
            abi.clobbered.is_empty(),
            "riscv64 ecall does not clobber additional registers"
        );
    }

    #[test]
    fn test_riscv64_syscall_numbers() {
        let abi = riscv64_abi();
        assert_eq!(abi.sys_write, 64);
        assert_eq!(abi.sys_read, 63);
        assert_eq!(abi.sys_close, 57);
        assert_eq!(abi.sys_exit, 93);
    }

    #[test]
    fn test_aarch64_riscv64_share_numbers() {
        let aarch64 = aarch64_abi();
        let riscv64 = riscv64_abi();
        assert_eq!(
            aarch64.sys_write, riscv64.sys_write,
            "aarch64 and riscv64 share asm-generic syscall numbers"
        );
        assert_eq!(aarch64.sys_read, riscv64.sys_read);
        assert_eq!(aarch64.sys_close, riscv64.sys_close);
        assert_eq!(aarch64.sys_exit, riscv64.sys_exit);
    }

    // ---- Real syscall tests (only run on Linux) ----

    #[cfg(target_os = "linux")]
    mod linux_tests {
        use super::*;

        #[test]
        fn test_sys_write_stdout() {
            let msg = b"[syscall_wrapper] sys_write test\n";
            let ret = sys_write(1, msg);
            assert_eq!(
                ret,
                msg.len() as isize,
                "sys_write should return bytes written"
            );
        }

        #[test]
        fn test_sys_write_stderr() {
            let msg = b"[syscall_wrapper] stderr test\n";
            let ret = sys_write(2, msg);
            assert_eq!(ret, msg.len() as isize);
        }

        #[test]
        fn test_sys_write_invalid_fd() {
            let ret = sys_write(999, b"hello");
            assert!(ret < 0, "invalid fd should return negative, got {ret}");
        }

        #[test]
        fn test_sys_close_invalid_fd() {
            let ret = sys_close(999);
            assert!(ret < 0, "closing invalid fd should return negative");
        }

        #[test]
        fn test_sys_read_and_write_socket_pair() {
            use std::os::fd::AsRawFd;
            use std::os::unix::net::UnixStream;

            let (reader, writer) = UnixStream::pair().unwrap();
            let message = b"read through syscall";
            assert_eq!(
                sys_write(writer.as_raw_fd() as usize, message),
                message.len() as isize
            );
            let mut buf = [0; 20];
            assert_eq!(
                sys_read(reader.as_raw_fd() as usize, &mut buf),
                message.len() as isize
            );
            assert_eq!(&buf, message);
        }

        #[test]
        fn test_sys_read_invalid_fd_preserves_buffer() {
            let mut buf = [0xAB; 4];
            assert!(sys_read(usize::MAX, &mut buf) < 0);
            assert_eq!(buf, [0xAB; 4]);
        }

        #[test]
        fn test_sys_close_owned_descriptor() {
            use std::os::fd::IntoRawFd;

            let fd = std::fs::File::open("/dev/null").unwrap().into_raw_fd();
            assert_eq!(sys_close(fd as usize), 0);
        }

        #[test]
        fn test_sys_exit_status_in_child() {
            use std::os::unix::process::CommandExt;

            let mut child = std::process::Command::new("/bin/true");
            // SAFETY: The child callback only invokes the async-signal-safe exit
            // syscall, before exec and while the forked child has one thread.
            unsafe { child.pre_exec(|| sys_exit(23)) };
            assert_eq!(child.status().unwrap().code(), Some(23));
        }
    }
}
