/*
Stackful coroutines for RISC-V Linux. Preserve sp, ra, and s0-s11 when
switching stacks; run the cross-compiled tests through QEMU on other hosts.
*/

#![cfg(target_arch = "riscv64")]

/// Saved register state; its layout must match the assembly offsets below.
/// `sp` at 0, `ra` at 8, then `s0`–`s11` at 16, 24, … 104.
#[repr(C)]
#[derive(Debug, Default, Clone, Copy)]
pub struct TaskContext {
    pub sp: u64,
    pub ra: u64,
    pub s0: u64,
    pub s1: u64,
    pub s2: u64,
    pub s3: u64,
    pub s4: u64,
    pub s5: u64,
    pub s6: u64,
    pub s7: u64,
    pub s8: u64,
    pub s9: u64,
    pub s10: u64,
    pub s11: u64,
}

impl TaskContext {
    pub const fn empty() -> Self {
        Self {
            sp: 0,
            ra: 0,
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            s4: 0,
            s5: 0,
            s6: 0,
            s7: 0,
            s8: 0,
            s9: 0,
            s10: 0,
            s11: 0,
        }
    }

    /// Initialize this context so that when we switch to it, execution starts at `entry`.
    ///
    /// - Set `ra = entry` so that the first `ret` in the new context jumps to `entry`.
    /// - Set `sp = stack_top` with 16-byte alignment (RISC-V ABI requires 16-byte aligned stack at function entry).
    /// - Leave `s0`–`s11` zero; they will be loaded on switch.
    pub fn init(&mut self, stack_top: usize, entry: usize) {
        *self = Self {
            sp: (stack_top & !15) as u64,
            ra: entry as u64,
            ..Self::empty()
        };
    }
}

/// Switch from `old` to `new` context: save current callee-saved regs into `old`, load from `new`, then `ret` (jumps to `new.ra`).
///
/// In asm: store `sp`, `ra`, `s0`–`s11` to `[a0]` (old), load from `[a1]` (new), zero `a0`/`a1` so we do not leak pointers into the new context, then `ret`.
///
/// Must be `#[unsafe(naked)]` to prevent the compiler from generating a prologue/epilogue.
///
/// # Safety
/// Both contexts must refer to live stacks. The destination must contain either
/// an initialized entry point or register state saved by an earlier switch.
/// Only the integer register state represented by `TaskContext` is preserved.
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(_old: *mut TaskContext, _new: *const TaskContext) {
    // A naked function keeps the saved stack pointer free of compiler frames.
    core::arch::naked_asm!(
        "sd sp, 0(a0)",
        "sd ra, 8(a0)",
        "sd s0, 16(a0)",
        "sd s1, 24(a0)",
        "sd s2, 32(a0)",
        "sd s3, 40(a0)",
        "sd s4, 48(a0)",
        "sd s5, 56(a0)",
        "sd s6, 64(a0)",
        "sd s7, 72(a0)",
        "sd s8, 80(a0)",
        "sd s9, 88(a0)",
        "sd s10, 96(a0)",
        "sd s11, 104(a0)",
        "ld sp, 0(a1)",
        "ld ra, 8(a1)",
        "ld s0, 16(a1)",
        "ld s1, 24(a1)",
        "ld s2, 32(a1)",
        "ld s3, 40(a1)",
        "ld s4, 48(a1)",
        "ld s5, 56(a1)",
        "ld s6, 64(a1)",
        "ld s7, 72(a1)",
        "ld s8, 80(a1)",
        "ld s9, 88(a1)",
        "ld s10, 96(a1)",
        "ld s11, 104(a1)",
        "li a0, 0",
        "li a1, 0",
        "ret",
    );
}

const STACK_SIZE: usize = 1024 * 64;

/// Allocate a stack for a coroutine. Returns `(buffer, stack_top)` where `stack_top` is the high address
/// (stack grows down). The buffer must be kept alive for the lifetime of the context using this stack.
pub fn alloc_stack() -> (Vec<u8>, usize) {
    let buffer = vec![0; STACK_SIZE];
    let stack_top = (buffer.as_ptr() as usize + buffer.len()) & !15;
    (buffer, stack_top)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    extern "C" fn task_entry() {
        COUNTER.store(42, Ordering::SeqCst);
        loop {
            std::hint::spin_loop();
        }
    }

    #[test]
    fn test_alloc_stack() {
        let (buf, top) = alloc_stack();
        assert_eq!(top, buf.as_ptr() as usize + STACK_SIZE);
        assert!(top % 16 == 0);
    }

    #[test]
    fn test_context_init() {
        let (buf, top) = alloc_stack();
        let _ = buf;
        let mut ctx = TaskContext::empty();
        let entry = task_entry as *const () as usize;
        ctx.init(top, entry);
        assert_eq!(ctx.ra, entry as u64);
        assert!(ctx.sp != 0);
    }

    #[test]
    fn test_switch_to_task() {
        COUNTER.store(0, Ordering::SeqCst);

        static mut MAIN_CTX_PTR: *mut TaskContext = std::ptr::null_mut();
        static mut TASK_CTX_PTR: *mut TaskContext = std::ptr::null_mut();

        extern "C" fn cooperative_task() {
            COUNTER.store(99, Ordering::SeqCst);
            unsafe {
                switch_context(&mut *TASK_CTX_PTR, &*MAIN_CTX_PTR);
            }
        }

        let (_stack_buf, stack_top) = alloc_stack();
        let mut main_ctx = TaskContext::empty();
        let mut task_ctx = TaskContext::empty();
        task_ctx.init(stack_top, cooperative_task as *const () as usize);

        unsafe {
            MAIN_CTX_PTR = &mut main_ctx;
            TASK_CTX_PTR = &mut task_ctx;
            switch_context(&mut main_ctx, &task_ctx);
        }

        assert_eq!(COUNTER.load(Ordering::SeqCst), 99);
    }

    #[test]
    fn test_resume_task_multiple_times() {
        static STEPS: AtomicU32 = AtomicU32::new(0);
        static mut MAIN: *mut TaskContext = std::ptr::null_mut();
        static mut TASK: *mut TaskContext = std::ptr::null_mut();

        extern "C" fn task() {
            for _ in 0..3 {
                STEPS.fetch_add(1, Ordering::SeqCst);
                // SAFETY: Both contexts and their stacks outlive every switch.
                unsafe { switch_context(&mut *TASK, &*MAIN) };
            }
        }

        let (_stack, top) = alloc_stack();
        let mut main = TaskContext::empty();
        let mut coroutine = TaskContext::empty();
        coroutine.init(top, task as *const () as usize);
        STEPS.store(0, Ordering::SeqCst);
        // SAFETY: The pointers remain valid until the coroutine is no longer resumed.
        unsafe {
            MAIN = &mut main;
            TASK = &mut coroutine;
            for expected in 1..=3 {
                switch_context(&mut main, &coroutine);
                assert_eq!(STEPS.load(Ordering::SeqCst), expected);
            }
            MAIN = std::ptr::null_mut();
            TASK = std::ptr::null_mut();
        }
    }
}
