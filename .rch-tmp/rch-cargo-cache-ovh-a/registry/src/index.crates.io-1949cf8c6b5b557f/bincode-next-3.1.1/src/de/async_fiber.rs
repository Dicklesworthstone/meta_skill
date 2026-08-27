#![allow(unsafe_code)]
#![allow(clippy::vec_box)]
// For Windows Development traditions, we allow non_snake_case
#![allow(non_snake_case)]
// Please notice this module do not support async operations that evolves AVX-256/AVX-512/AArch64 SVE/RISC-V V, and if evolved, shall be saved by the caller per ABI standards.

#[cfg(feature = "async-fiber")]
use alloc::boxed::Box;
#[cfg(feature = "async-fiber")]
use alloc::vec::Vec;
#[cfg(feature = "async-fiber")]
use core::pin::Pin;
#[cfg(feature = "async-fiber")]
use std::arch::naked_asm;
#[cfg(feature = "async-fiber")]
use std::cell::RefCell;
#[cfg(feature = "async-fiber")]
use std::future::Future;
#[cfg(feature = "async-fiber")]
use std::task::Context;
#[cfg(feature = "async-fiber")]
use std::task::Poll;

/// Machine-specific registers for context switching.
#[cfg(feature = "async-fiber")]
#[repr(C, align(64))]
#[derive(Debug)]
pub(crate) struct Registers {
    /// General purpose registers.
    pub(crate) gprs: [u64; 16],
    /// Extended state (e.g., FPU/SIMD for `x86_64`, NEON for Aarch64, fp for RISC-V).
    pub(crate) extended_state: [u8; 512],
}

#[cfg(feature = "async-fiber")]
impl Registers {
    /// Create a zero-initialized register state.
    #[must_use]
    #[inline(always)]
    pub(crate) const fn new() -> Self {
        Self {
            gprs: [0; 16],
            extended_state: [0; 512],
        }
    }
}

impl Default for Registers {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}

/// Represents the current execution status of a Fiber.
#[cfg(feature = "async-fiber")]
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum FiberStatus {
    /// Fiber has been allocated but not yet started executing.
    Initial,
    /// Fiber is currently executing or ready to execute.
    Running,
    /// Fiber was suspended while waiting for additional async I/O data.
    Yielded,
    /// Fiber has completed its closure execution.
    Finished,
    /// Fiber experienced a panic that was caught.
    Panicked,
}

/// A fiber stack with an `mmap`-backed guard page at the bottom.
///
/// Layout (low → high):
/// ```text
/// [ guard page  |  usable stack memory ]
///   PAGE_SIZE       stack_size
/// ```
///
/// The guard page is mapped `PROT_NONE`, so any access (stack overflow) will
/// trigger a hardware fault (SIGSEGV / SIGBUS) instead of silently corrupting
/// adjacent heap memory.
#[cfg(feature = "async-fiber")]
#[allow(dead_code)]
pub(crate) struct GuardedStack {
    /// Base pointer returned by `mmap` (start of guard page).
    base: *mut u8,
    /// Total allocation length (guard + usable).
    total_len: usize,
    /// Page size used for the guard.
    page_size: usize,
}

#[cfg(feature = "async-fiber")]
impl GuardedStack {
    /// Allocate a new guarded stack of *at least* `usable_size` bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if `mmap` or `mprotect` fails.
    #[inline]
    #[cfg(unix)]
    pub(crate) fn new(usable_size: usize) -> core::result::Result<Self, crate::error::DecodeError> {
        let page_size = page_size();
        // Round usable_size up to page boundary.
        let usable_size = (usable_size + page_size - 1) & !(page_size - 1);
        let total_len = page_size + usable_size;

        unsafe {
            let base = libc::mmap(
                core::ptr::null_mut(),
                total_len,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            );
            if base == libc::MAP_FAILED {
                return crate::error::cold_decode_error_other("mmap failed for fiber stack");
            }

            // Protect the first page as a guard (PROT_NONE → any access faults).
            let rc = libc::mprotect(base, page_size, libc::PROT_NONE);
            if rc != 0 {
                libc::munmap(base, total_len);
                return crate::error::cold_decode_error_other("mprotect failed for guard page");
            }

            Ok(Self {
                base: base.cast::<u8>(),
                total_len,
                page_size,
            })
        }
    }

    /// Allocate a new guarded stack on Windows.
    ///
    /// # Errors
    ///
    /// Returns an error if `VirtualAlloc` or `VirtualProtect` fails.
    #[inline]
    #[cfg(windows)]
    pub(crate) fn new(usable_size: usize) -> core::result::Result<Self, crate::error::DecodeError> {
        let page_size = page_size();
        let usable_size = (usable_size + page_size - 1) & !(page_size - 1);
        let total_len = page_size + usable_size;

        unsafe {
            // Reserve and commit the entire stack
            let base = winapi_shim::VirtualAlloc(
                core::ptr::null_mut(),
                total_len,
                winapi_shim::MEM_COMMIT | winapi_shim::MEM_RESERVE,
                winapi_shim::PAGE_READWRITE,
            );
            if base.is_null() {
                return crate::error::cold_decode_error_other(
                    "VirtualAlloc failed for fiber stack",
                );
            }

            // Guard the first page
            let mut old_protect = 0;
            let rc = winapi_shim::VirtualProtect(
                base,
                page_size,
                winapi_shim::PAGE_NOACCESS,
                &raw mut old_protect,
            );
            if rc == 0 {
                winapi_shim::VirtualFree(base, 0, winapi_shim::MEM_RELEASE);
                return crate::error::cold_decode_error_other(
                    "VirtualProtect failed for guard page",
                );
            }

            Ok(Self {
                base: base.cast::<u8>(),
                total_len,
                page_size,
            })
        }
    }

    /// Usable stack region (excludes guard page).
    #[inline(always)]
    #[must_use]
    #[allow(dead_code)]
    pub(crate) const fn usable(&self) -> &[u8] {
        unsafe {
            core::slice::from_raw_parts(
                self.base.add(self.page_size),
                self.total_len - self.page_size,
            )
        }
    }

    /// Usable stack region (mutable).
    #[inline(always)]
    #[allow(dead_code)]
    pub(crate) const fn usable_mut(&mut self) -> &mut [u8] {
        unsafe {
            core::slice::from_raw_parts_mut(
                self.base.add(self.page_size),
                self.total_len - self.page_size,
            )
        }
    }

    /// The top of the usable stack (highest address), 16-byte aligned.
    #[inline(always)]
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn top(&self) -> u64 {
        let raw = self.base as u64 + self.total_len as u64;
        raw & !15 // 16-byte align
    }

    /// The bottom of the usable stack (lowest usable address, just above guard page).
    #[inline(always)]
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn bottom(&self) -> u64 {
        self.base as u64 + self.page_size as u64
    }

    /// The absolute base of the mapping (potentially including guard page).
    #[inline(always)]
    #[must_use]
    #[allow(dead_code)]
    pub(crate) fn allocation_base(&self) -> u64 {
        self.base as u64
    }
}

#[cfg(feature = "async-fiber")]
impl Drop for GuardedStack {
    #[inline(always)]
    #[cfg(unix)]
    fn drop(&mut self) {
        unsafe {
            let rc = libc::munmap(self.base.cast::<libc::c_void>(), self.total_len);
            debug_assert!(rc == 0, "munmap failed for fiber stack");
        }
    }

    #[inline(always)]
    #[cfg(windows)]
    fn drop(&mut self) {
        unsafe {
            let rc = winapi_shim::VirtualFree(
                self.base.cast::<core::ffi::c_void>(),
                0, // dwSize must be 0 for MEM_RELEASE
                winapi_shim::MEM_RELEASE,
            );
            debug_assert!(rc != 0, "VirtualFree failed for fiber stack");
        }
    }
}

// GuardedStack owns a unique mmap region — safe to move between threads.
#[cfg(feature = "async-fiber")]
unsafe impl Send for GuardedStack {}

#[cfg(all(feature = "async-fiber", unix))]
#[inline(always)]
fn page_size() -> usize {
    // Cached via a static to avoid repeated syscalls.
    static PAGE_SIZE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *PAGE_SIZE.get_or_init(|| unsafe { libc::sysconf(libc::_SC_PAGESIZE) as usize })
}

#[cfg(all(feature = "async-fiber", windows))]
#[inline(always)]
fn page_size() -> usize {
    static PAGE_SIZE: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *PAGE_SIZE.get_or_init(|| {
        let mut info = core::mem::MaybeUninit::uninit();
        unsafe {
            winapi_shim::GetSystemInfo(info.as_mut_ptr());
            info.assume_init().dwPageSize as usize
        }
    })
}

#[cfg(all(feature = "async-fiber", windows))]
mod winapi_shim {
    #[repr(C)]
    pub struct SYSTEM_INFO {
        pub wProcessorArchitecture: u16,
        pub wReserved: u16,
        pub dwPageSize: u32,
        pub lpMinimumApplicationAddress: *mut core::ffi::c_void,
        pub lpMaximumApplicationAddress: *mut core::ffi::c_void,
        pub dwActiveProcessorMask: usize,
        pub dwNumberOfProcessors: u32,
        pub dwProcessorType: u32,
        pub dwAllocationGranularity: u32,
        pub wProcessorLevel: u16,
        pub wProcessorRevision: u16,
    }
    pub const MEM_COMMIT: u32 = 0x0000_1000;
    pub const MEM_RESERVE: u32 = 0x0000_2000;
    pub const MEM_RELEASE: u32 = 0x0000_8000;
    pub const PAGE_NOACCESS: u32 = 0x01;
    pub const PAGE_READWRITE: u32 = 0x04;

    unsafe extern "system" {
        pub fn VirtualAlloc(
            lpAddress: *mut core::ffi::c_void,
            dwSize: usize,
            flAllocationType: u32,
            flProtect: u32,
        ) -> *mut core::ffi::c_void;
        pub fn VirtualFree(
            lpAddress: *mut core::ffi::c_void,
            dwSize: usize,
            dwFreeType: u32,
        ) -> i32;
        pub fn VirtualProtect(
            lpAddress: *mut core::ffi::c_void,
            dwSize: usize,
            flNewProtect: u32,
            lpflOldProtect: *mut u32,
        ) -> i32;
        pub fn GetSystemInfo(lpSystemInfo: *mut SYSTEM_INFO);
    }
}

/// Context metadata for an executing fiber, managing stacks, registers, and closure passing.
#[cfg(feature = "async-fiber")]
#[repr(C, align(16))]
pub(crate) struct FiberContext {
    /// The `GuardedStack` which maps the actual stack space in memory.
    pub(crate) stack: GuardedStack,
    /// Registers for the fiber state.
    pub(crate) regs: Registers,
    /// Registers for the executor state (where the fiber yields back to).
    pub(crate) executor_regs: Registers,
    /// The lifecycle status of the fiber.
    pub(crate) status: FiberStatus,
    /// Holds panic payload if a panic occurred within the fiber.
    pub(crate) panic_payload: Option<Box<dyn std::any::Any + Send>>,
    /// Fixed landing assembly to launch closures.
    pub(crate) trampoline: unsafe extern "C" fn(),
    /// Closure thunk invocation pointer.
    pub(crate) invoke_closure: unsafe fn(*mut ()),
    /// Opaque pointer to closure state.
    pub(crate) closure_ptr: *mut (),
    /// Opaque pointer to the result slot.
    pub(crate) result_ptr: *mut (),
    /// Opaque pointer to the `AsyncRead` structure.
    pub(crate) reader_ptr: *mut (),
    /// Byte slice actively being used as data source for parsing.
    pub(crate) buf_ptr: *mut [u8],
    /// 8KB heap-allocated IO staging buffer.
    pub(crate) read_buffer: Box<[u8]>,
}

// FiberContext is intentionally NOT Send/Sync.
// It is only accessed through BridgeFuture, which manually implements
// Send/Sync with the correct safety invariants.
#[cfg(feature = "async-fiber")]
std::thread_local! {
    static CONTEXT_POOL: RefCell<Vec<Box<FiberContext>>> = const { RefCell::new(Vec::new()) };
    static CURRENT_FIBER: std::cell::Cell<*mut FiberContext> = const { std::cell::Cell::new(core::ptr::null_mut()) };
}

/// Cap the number of contexts pooled per thread to prevent unbounded mmap accumulation.
#[cfg(feature = "async-fiber")]
const MAX_POOLED_CONTEXTS: usize = 8_192;

// x86_64 context switch — System V ABI (Linux, macOS, FreeBSD, …)
// Arguments: rdi = save, rsi = restore
#[cfg(all(feature = "async-fiber", target_arch = "x86_64", unix))]
#[unsafe(naked)]
unsafe extern "C" fn switch_context(
    save: *mut Registers,
    restore: *const Registers,
) {
    naked_asm!(
        "prefetchw [rdi]",
        "prefetcht0 [rsi]",
        "prefetcht0 [rsi + 64]",
        "prefetcht0 [rsi + 128]",
        "mov rax, [rsi]",
        "prefetcht0 [rax]",
        "prefetcht0 [rax + 64]",
        "mov [rdi + 0], rsp",
        "mov [rdi + 8], rbp",
        "mov [rdi + 16], rbx",
        "mov [rdi + 24], r12",
        "mov [rdi + 32], r13",
        "mov [rdi + 40], r14",
        "mov [rdi + 48], r15",
        "stmxcsr [rdi + 64]",
        "lea rax, [rip + 1f]",
        "mov [rdi + 56], rax",
        "ldmxcsr [rsi + 64]",
        "mov rsp, [rsi + 0]",
        "mov rbp, [rsi + 8]",
        "mov rbx, [rsi + 16]",
        "mov r12, [rsi + 24]",
        "mov r13, [rsi + 32]",
        "mov r14, [rsi + 40]",
        "mov r15, [rsi + 48]",
        "jmp [rsi + 56]",
        "1: ret"
    );
}

// x86_64 context switch — Microsoft x64 ABI (Windows)
// Arguments: rcx = save, rdx = restore
// Additional: rdi/rsi are callee-saved; TIB stack bounds must be swapped.
//   gprs[8]  = rdi      gprs[9]  = rsi
//   gprs[10] = TIB StackBase      (gs:[0x08])
//   gprs[11] = TIB StackLimit     (gs:[0x10])
//   gprs[12] = TIB DeallocationStack (gs:[0x1478])
//   gprs[13] = TIB ExceptionList  (gs:[0x00])
#[cfg(all(feature = "async-fiber", target_arch = "x86_64", windows))]
#[unsafe(naked)]
unsafe extern "C" fn switch_context(
    save: *mut Registers,
    restore: *const Registers,
) {
    naked_asm!(
        "prefetchw [rcx]",
        "prefetcht0 [rdx]",
        "prefetcht0 [rdx + 64]",
        "prefetcht0 [rdx + 128]",
        "mov rax, [rdx]",
        "prefetcht0 [rax]",
        "prefetcht0 [rax + 64]",
        "mov [rcx + 0], rsp",
        "mov [rcx + 8], rbp",
        "mov [rcx + 16], rbx",
        "mov [rcx + 24], r12",
        "mov [rcx + 32], r13",
        "mov [rcx + 40], r14",
        "mov [rcx + 48], r15",
        "mov [rcx + 64], rdi",
        "mov [rcx + 72], rsi",
        "mov rax, gs:[0x08]",
        "mov [rcx + 80], rax",
        "mov rax, gs:[0x10]",
        "mov [rcx + 88], rax",
        "mov rax, gs:[0x1478]",
        "mov [rcx + 96], rax",
        "mov rax, gs:[0x00]",
        "mov [rcx + 104], rax",
        "stmxcsr [rcx + 112]",
        "movaps [rcx + 128], xmm6",
        "movaps [rcx + 144], xmm7",
        "movaps [rcx + 160], xmm8",
        "movaps [rcx + 176], xmm9",
        "movaps [rcx + 192], xmm10",
        "movaps [rcx + 208], xmm11",
        "movaps [rcx + 224], xmm12",
        "movaps [rcx + 240], xmm13",
        "movaps [rcx + 256], xmm14",
        "movaps [rcx + 272], xmm15",
        "lea rax, [rip + 1f]",
        "mov [rcx + 56], rax",
        "movaps xmm6,  [rdx + 128]",
        "movaps xmm7,  [rdx + 144]",
        "movaps xmm8,  [rdx + 160]",
        "movaps xmm9,  [rdx + 176]",
        "movaps xmm10, [rdx + 192]",
        "movaps xmm11, [rdx + 208]",
        "movaps xmm12, [rdx + 224]",
        "movaps xmm13, [rdx + 240]",
        "movaps xmm14, [rdx + 256]",
        "movaps xmm15, [rdx + 272]",
        "ldmxcsr [rdx + 112]",
        "mov rax, [rdx + 80]",
        "mov gs:[0x08], rax",
        "mov rax, [rdx + 88]",
        "mov gs:[0x10], rax",
        "mov rax, [rdx + 96]",
        "mov gs:[0x1478], rax",
        "mov rax, [rdx + 104]",
        "mov gs:[0x00], rax",
        "mov rsp, [rdx + 0]",
        "mov rbp, [rdx + 8]",
        "mov rbx, [rdx + 16]",
        "mov r12, [rdx + 24]",
        "mov r13, [rdx + 32]",
        "mov r14, [rdx + 40]",
        "mov r15, [rdx + 48]",
        "mov rdi, [rdx + 64]",
        "mov rsi, [rdx + 72]",
        "jmp [rdx + 56]",
        "1: ret"
    );
}

// aarch64 context switch — AAPCS64 (Linux, macOS, FreeBSD)
// Arguments: x0 = save, x1 = restore
#[cfg(all(
    feature = "async-fiber",
    target_arch = "aarch64",
    unix,
    target_os = "macos"
))]
#[unsafe(naked)]
unsafe extern "C" fn switch_context(
    save: *mut Registers,
    restore: *const Registers,
) {
    naked_asm!(
        "bti c",
        "prfm pstl1keep, [x0]",
        "prfm pldl1keep, [x1]",
        "prfm pldl1keep, [x1, 64]",
        "prfm pldl1keep, [x1, 128]",
        "ldr x9, [x1, 96]",
        "prfm pldl1keep, [x9]",
        "prfm pldl1keep, [x9, 64]",
        "stp x19, x20, [x0, 0]",
        "stp x21, x22, [x0, 16]",
        "stp x23, x24, [x0, 32]",
        "stp x25, x26, [x0, 48]",
        "stp x27, x28, [x0, 64]",
        "stp x29, x30, [x0, 80]",
        "mov x9, sp",
        "str x9, [x0, 96]",
        "stp d8,  d9,  [x0, 128]",
        "stp d10, d11, [x0, 144]",
        "stp d12, d13, [x0, 160]",
        "stp d14, d15, [x0, 176]",
        "ldp d8,  d9,  [x1, 128]",
        "ldp d10, d11, [x1, 144]",
        "ldp d12, d13, [x1, 160]",
        "ldp d14, d15, [x1, 176]",
        "ldp x19, x20, [x1, 0]",
        "ldp x21, x22, [x1, 16]",
        "ldp x23, x24, [x1, 32]",
        "ldp x25, x26, [x1, 48]",
        "ldp x27, x28, [x1, 64]",
        "ldp x29, x30, [x1, 80]",
        "ldr x9, [x1, 96]",
        "mov sp, x9",
        "ret"
    );
}

// aarch64 context switch — AAPCS64 (Linux, macOS, FreeBSD)
// Arguments: x0 = save, x1 = restore
#[cfg(all(
    feature = "async-fiber",
    target_arch = "aarch64",
    unix,
    not(target_os = "macos")
))]
#[unsafe(naked)]
unsafe extern "C" fn switch_context(
    save: *mut Registers,
    restore: *const Registers,
) {
    naked_asm!(
        "prfm pstl1keep, [x0]",
        "prfm pldl1keep, [x1]",
        "prfm pldl1keep, [x1, 64]",
        "prfm pldl1keep, [x1, 128]",
        "ldr x9, [x1, 96]",
        "prfm pldl1keep, [x9]",
        "prfm pldl1keep, [x9, 64]",
        "stp x19, x20, [x0, 0]",
        "stp x21, x22, [x0, 16]",
        "stp x23, x24, [x0, 32]",
        "stp x25, x26, [x0, 48]",
        "stp x27, x28, [x0, 64]",
        "stp x29, x30, [x0, 80]",
        "mov x9, sp",
        "str x9, [x0, 96]",
        "stp d8,  d9,  [x0, 128]",
        "stp d10, d11, [x0, 144]",
        "stp d12, d13, [x0, 160]",
        "stp d14, d15, [x0, 176]",
        "ldp x19, x20, [x1, 0]",
        "ldp x21, x22, [x1, 16]",
        "ldp x23, x24, [x1, 32]",
        "ldp x25, x26, [x1, 48]",
        "ldp x27, x28, [x1, 64]",
        "ldp x29, x30, [x1, 80]",
        "ldr x9, [x1, 96]",
        "mov sp, x9",
        "ldp d8,  d9,  [x1, 128]",
        "ldp d10, d11, [x1, 144]",
        "ldp d12, d13, [x1, 160]",
        "ldp d14, d15, [x1, 176]",
        "ret"
    );
}

// aarch64 context switch — Windows ARM64
// Arguments: x0 = save, x1 = restore
// Additional: TEB is at x18; must swap stack bounds.
//   gprs[13] = TEB StackBase      (x18 + 0x08)
//   gprs[14] = TEB StackLimit     (x18 + 0x10)
//   gprs[15] = TEB DeallocationStack (x18 + 0x1478)
#[cfg(all(feature = "async-fiber", target_arch = "aarch64", windows))]
#[unsafe(naked)]
unsafe extern "C" fn switch_context(
    save: *mut Registers,
    restore: *const Registers,
) {
    naked_asm!(
        "bti c",
        "prfm pstl1keep, [x0]",
        "prfm pldl1keep, [x1]",
        "prfm pldl1keep, [x1, 64]",
        "prfm pldl1keep, [x1, 128]",
        "ldr x9, [x1, 96]",
        "prfm pldl1keep, [x9]",
        "prfm pldl1keep, [x9, 64]",
        "stp x19, x20, [x0, 0]",
        "stp x21, x22, [x0, 16]",
        "stp x23, x24, [x0, 32]",
        "stp x25, x26, [x0, 48]",
        "stp x27, x28, [x0, 64]",
        "stp x29, x30, [x0, 80]",
        "mov x9, sp",
        "str x9, [x0, 96]",
        "ldr x9, [x18, #0x08]",
        "str x9, [x0, #104]",
        "ldr x9, [x18, #0x10]",
        "str x9, [x0, #112]",
        "ldr x9, [x18, #0x12C8]",
        "str x9, [x0, #120]",
        "stp d8,  d9,  [x0, 128]",
        "stp d10, d11, [x0, 144]",
        "stp d12, d13, [x0, 160]",
        "stp d14, d15, [x0, 176]",
        "ldp d8,  d9,  [x1, 128]",
        "ldp d10, d11, [x1, 144]",
        "ldp d12, d13, [x1, 160]",
        "ldp d14, d15, [x1, 176]",
        "ldr x9, [x1, #104]",
        "str x9, [x18, #0x08]",
        "ldr x9, [x1, #112]",
        "str x9, [x18, #0x10]",
        "ldr x9, [x1, #120]",
        "str x9, [x18, #0x12C8]",
        "ldp x19, x20, [x1, 0]",
        "ldp x21, x22, [x1, 16]",
        "ldp x23, x24, [x1, 32]",
        "ldp x25, x26, [x1, 48]",
        "ldp x27, x28, [x1, 64]",
        "ldp x29, x30, [x1, 80]",
        "ldr x9, [x1, 96]",
        "mov sp, x9",
        "ret"
    );
}

// riscv64 context switch — unix only (no Windows RISC-V target exists)
#[cfg(all(
    feature = "async-fiber",
    target_arch = "riscv64",
    unix,
    not(feature = "hw-acceleration")
))]
#[unsafe(naked)]
unsafe extern "C" fn switch_context(
    save: *mut Registers,
    restore: *const Registers,
) {
    naked_asm!(
        "sd sp, 0(a0)",
        "sd s0, 8(a0)",
        "sd s1, 16(a0)",
        "sd s2, 24(a0)",
        "sd s3, 32(a0)",
        "sd s4, 40(a0)",
        "sd s5, 48(a0)",
        "sd s6, 56(a0)",
        "sd s7, 64(a0)",
        "sd s8, 72(a0)",
        "sd s9, 80(a0)",
        "sd s10, 88(a0)",
        "sd s11, 96(a0)",
        "sd ra, 104(a0)",
        "fsd fs0, 128(a0)",
        "fsd fs1, 136(a0)",
        "fsd fs2, 144(a0)",
        "fsd fs3, 152(a0)",
        "fsd fs4, 160(a0)",
        "fsd fs5, 168(a0)",
        "fsd fs6, 176(a0)",
        "fsd fs7, 184(a0)",
        "fsd fs8, 192(a0)",
        "fsd fs9, 200(a0)",
        "fsd fs10, 208(a0)",
        "fsd fs11, 216(a0)",
        "ld sp, 0(a1)",
        "ld s0, 8(a1)",
        "ld s1, 16(a1)",
        "ld s2, 24(a1)",
        "ld s3, 32(a1)",
        "ld s4, 40(a1)",
        "ld s5, 48(a1)",
        "ld s6, 56(a1)",
        "ld s7, 64(a1)",
        "ld s8, 72(a1)",
        "ld s9, 80(a1)",
        "ld s10, 88(a1)",
        "ld s11, 96(a1)",
        "ld ra, 104(a1)",
        "fld fs0, 128(a1)",
        "fld fs1, 136(a1)",
        "fld fs2, 144(a1)",
        "fld fs3, 152(a1)",
        "fld fs4, 160(a1)",
        "fld fs5, 168(a1)",
        "fld fs6, 176(a1)",
        "fld fs7, 184(a1)",
        "fld fs8, 192(a1)",
        "fld fs9, 200(a1)",
        "fld fs10, 208(a1)",
        "fld fs11, 216(a1)",
        "ret"
    );
}

// riscv64 context switch — unix only (no Windows RISC-V target exists)
#[cfg(all(
    feature = "async-fiber",
    target_arch = "riscv64",
    unix,
    feature = "hw-acceleration"
))]
#[unsafe(naked)]
unsafe extern "C" fn switch_context(
    save: *mut Registers,
    restore: *const Registers,
) {
    naked_asm!(
        "prefetch.w 0(a0)",
        "prefetch.r 0(a1)",
        "prefetch.r 64(a1)",
        "prefetch.r 128(a1)",
        "ld a2, 0(a1)",
        "prefetch.r 0(a2)",
        "prefetch.r 64(a2)",
        "sd sp, 0(a0)",
        "sd s0, 8(a0)",
        "sd s1, 16(a0)",
        "sd s2, 24(a0)",
        "sd s3, 32(a0)",
        "sd s4, 40(a0)",
        "sd s5, 48(a0)",
        "sd s6, 56(a0)",
        "sd s7, 64(a0)",
        "sd s8, 72(a0)",
        "sd s9, 80(a0)",
        "sd s10, 88(a0)",
        "sd s11, 96(a0)",
        "sd ra, 104(a0)",
        "fsd fs0, 128(a0)",
        "fsd fs1, 136(a0)",
        "fsd fs2, 144(a0)",
        "fsd fs3, 152(a0)",
        "fsd fs4, 160(a0)",
        "fsd fs5, 168(a0)",
        "fsd fs6, 176(a0)",
        "fsd fs7, 184(a0)",
        "fsd fs8, 192(a0)",
        "fsd fs9, 200(a0)",
        "fsd fs10, 208(a0)",
        "fsd fs11, 216(a0)",
        "ld sp, 0(a1)",
        "ld s0, 8(a1)",
        "ld s1, 16(a1)",
        "ld s2, 24(a1)",
        "ld s3, 32(a1)",
        "ld s4, 40(a1)",
        "ld s5, 48(a1)",
        "ld s6, 56(a1)",
        "ld s7, 64(a1)",
        "ld s8, 72(a1)",
        "ld s9, 80(a1)",
        "ld s10, 88(a1)",
        "ld s11, 96(a1)",
        "ld ra, 104(a1)",
        "fld fs0, 128(a1)",
        "fld fs1, 136(a1)",
        "fld fs2, 144(a1)",
        "fld fs3, 152(a1)",
        "fld fs4, 160(a1)",
        "fld fs5, 168(a1)",
        "fld fs6, 176(a1)",
        "fld fs7, 184(a1)",
        "fld fs8, 192(a1)",
        "fld fs9, 200(a1)",
        "fld fs10, 208(a1)",
        "fld fs11, 216(a1)",
        "ret"
    );
}

#[cfg(all(
    feature = "async-fiber",
    not(any(
        all(target_arch = "x86_64", any(unix, windows)),
        all(target_arch = "aarch64", any(unix, windows)),
        all(target_arch = "riscv64", unix)
    ))
))]
compile_error!(
    "Unified Fiber-backed Async (async-fiber) is supported on: x86_64 (unix/windows), aarch64 (unix/windows), riscv64 (unix only)."
);

/// A synchronus `bincode::de::read::Reader` implementation that runs entirely inside a Fiber stack.
/// Implicitly yields execution back to the root event loop context if not enough bytes are available.
#[cfg(feature = "async-fiber")]
pub(crate) struct FiberReader<'a, R: futures_io::AsyncRead + Unpin> {
    /// Phanton marker tracking the inner lifetime.
    pub(crate) inner: std::marker::PhantomData<&'a mut R>,
    /// Context pointer to read bounds or swap threads.
    pub(crate) ctx: *mut FiberContext,
}

#[cfg(feature = "async-fiber")]
impl<R: futures_io::AsyncRead + Unpin> crate::de::read::Reader for FiberReader<'_, R> {
    #[inline]
    fn read(
        &mut self,
        bytes: &mut [u8],
    ) -> Result<(), crate::error::DecodeError> {
        let n = bytes.len();
        let mut written = 0;
        let ctx = unsafe { &mut *self.ctx };
        while written < n {
            let buf = unsafe { &mut *ctx.buf_ptr };

            if buf.is_empty() {
                unsafe {
                    ctx.status = FiberStatus::Yielded;
                    switch_context(&raw mut ctx.regs, &raw const ctx.executor_regs);

                    if ctx.status == FiberStatus::Finished {
                        return crate::error::cold_decode_error_unexpected_end(n - written);
                    }
                }
            }

            let buf = unsafe { &mut *ctx.buf_ptr };

            if buf.is_empty() {
                return crate::error::cold_decode_error_unexpected_end(n - written);
            }
            let to_copy = core::cmp::min(n - written, buf.len());
            bytes[written..written + to_copy].copy_from_slice(&buf[0..to_copy]);

            unsafe {
                ctx.buf_ptr = core::ptr::slice_from_raw_parts_mut(
                    buf.as_mut_ptr().add(to_copy),
                    buf.len() - to_copy,
                );
            }

            written += to_copy;
        }
        Ok(())
    }
}

/// Standard entry point that allows asynchronously decoding structs by transparently spawning an executor-integrated Fiber state machine.
///
/// # CRITICAL SAFETY WARNING: THREAD MIGRATION & TLS
///
/// Under standard usage, `BridgeFuture` implements `Send` allowing execution on
/// multi-threaded runtimes (like Tokio's worker pool).
///
/// **DO NOT use Thread-Local Storage (TLS) or `!Send` types (e.g. `Rc`, `RefCell`, `MutexGuard`) inside your `Decode` trait implementations!**
///
/// When the underlying reader returns `Poll::Pending`, the fiber execution stack
/// is suspended. A work-stealing executor may then migrate the suspended `BridgeFuture`
/// to a completely different OS thread.
///
/// Normally, the Rust compiler analyzes `.await` points and prevents futures holding
/// `!Send` data from implementing `Send`. However, because the fiber's yield point is
/// hidden behind the synchronous `bincode::de::read::Reader::read` invocation, the
/// compiler **cannot** analyze the variables held on the fiber's stack.
///
/// If a fiber migrates threads while holding a reference to Thread **A**'s TLS,
/// upon resuming, it will execute on Thread **B** while still illegally referencing
/// Thread **A**'s memory buffer, resulting in **Undefined Behavior**, panicking, or
/// silent memory corruption.
#[cfg(feature = "async-fiber")]
pub(crate) struct AsyncFiberBridge<R: futures_io::AsyncRead + Unpin> {
    /// Underlying futures_io-based `AsyncRead` source.
    pub(crate) reader: R,
}

#[cfg(feature = "async-fiber")]
impl<R: futures_io::AsyncRead + Unpin> AsyncFiberBridge<R> {
    /// Constructs a new asynchronous bridge mapping `futures_io`'s `AsyncRead`.
    #[inline(always)]
    pub(crate) const fn new(reader: R) -> Self {
        Self { reader }
    }

    /// Spawns the parsing process, converting the synchronous Decode traits to a Future.
    #[inline(always)]
    pub(crate) fn run<F, T>(
        self,
        f: F,
    ) -> impl Future<Output = Result<T, crate::error::DecodeError>>
    where
        F: FnOnce(&mut FiberReader<'_, R>) -> Result<T, crate::error::DecodeError>,
    {
        BridgeFuture {
            reader: self.reader,
            f: Some(f),
            ctx: None,
            result: None,
            _marker: core::marker::PhantomData,
        }
    }
}

#[cfg(feature = "async-fiber")]
#[inline(always)]
const unsafe fn dummy_invoke(_: *mut ()) {}

#[cfg(feature = "async-fiber")]
#[inline]
unsafe extern "C" fn fiber_trampoline() {
    unsafe {
        let ctx_ptr = CURRENT_FIBER.with(core::cell::Cell::get);
        let ctx = &mut *ctx_ptr;

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (ctx.invoke_closure)(ctx.closure_ptr);
        }));

        ctx.status = if let Err(e) = result {
            ctx.panic_payload = Some(e);
            FiberStatus::Panicked
        } else {
            FiberStatus::Finished
        };

        // Clear the thread-local before switching back — the fiber is done.
        CURRENT_FIBER.with(|c| c.set(core::ptr::null_mut()));
        switch_context(&raw mut ctx.regs, &raw const ctx.executor_regs);
        unreachable!("fiber finished and should not be resumed");
    }
}

/// Helper: set the thread-local and assert thread affinity, then switch.
///
/// # Safety
/// `ctx` must be a valid, pinned `FiberContext` whose stack is live.
#[cfg(feature = "async-fiber")]
#[inline]
unsafe fn resume_fiber(ctx: &mut FiberContext) {
    unsafe {
        // Thread affinity check is completely omitted; fibers can transparently
        // migrate across executor threads since `mmap` and heap regions share
        // the generic virtual address space and executor generic regs snapshot
        // the active execution context per poll dynamically.

        CURRENT_FIBER.with(|c| c.set(core::ptr::from_mut(ctx)));
        switch_context(&raw mut ctx.executor_regs, &raw const ctx.regs);
        // After returning here the fiber has yielded or finished.
        // Clear the thread-local to prevent stale pointer access.
        CURRENT_FIBER.with(|c| c.set(core::ptr::null_mut()));
    }
}

#[cfg(feature = "async-fiber")]
struct BridgeFuture<R, F, T> {
    reader: R,
    f: Option<F>,
    ctx: Option<Box<FiberContext>>,
    result: Option<Result<T, crate::error::DecodeError>>,
    _marker: core::marker::PhantomData<T>,
}

// SAFETY: BridgeFuture is Send+Sync when its components are.
//
// WARNING: As stated on `AsyncFiberBridge`, this circumvents the compiler's
// ability to analyze variables held across yield points. The fiber natively
// guarantees pointer safety for generic hardware execution bounds via the `mmap`
// generic process address space and dynamic `executor_regs` restoring. However,
// `!Send` structures (like `Rc` and `thread_local!`) instantiated inside the
// user's `Decode` stack will blindly be migrated across threads, which is **UB**.
//
// By using this abstraction, the caller is trusted that their `Decode` derivations
// strictly instantiate thread-safe, `Send`-equivalent variables locally.
#[cfg(feature = "async-fiber")]
#[allow(clippy::non_send_fields_in_send_ty)]
unsafe impl<R: Send, F: Send, T: Send> Send for BridgeFuture<R, F, T> {}
#[cfg(feature = "async-fiber")]
unsafe impl<R: Sync, F: Sync, T: Sync> Sync for BridgeFuture<R, F, T> {}

#[cfg(feature = "async-fiber")]
impl<R, F, T> Future for BridgeFuture<R, F, T>
where
    R: futures_io::AsyncRead + Unpin,
    F: FnOnce(&mut FiberReader<'_, R>) -> Result<T, crate::error::DecodeError>,
{
    type Output = Result<T, crate::error::DecodeError>;

    #[allow(clippy::too_many_lines)]
    fn poll(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Self::Output> {
        // -- Initialise fiber on first poll ----------------------------------
        if self.ctx.is_none() {
            let mut ctx = if let Some(ctx) = CONTEXT_POOL.with(|pool| pool.borrow_mut().pop()) {
                ctx
            } else {
                // Default 2 MiB usable stack via mmap with guard region.
                match GuardedStack::new(2 * 1024 * 1024) {
                    | Ok(stack) => {
                        Box::new(FiberContext {
                            stack,
                            regs: Registers::new(),
                            executor_regs: Registers::new(),
                            status: FiberStatus::Initial,
                            panic_payload: None,
                            trampoline: fiber_trampoline,
                            invoke_closure: dummy_invoke,
                            closure_ptr: core::ptr::null_mut(),
                            result_ptr: core::ptr::null_mut(),
                            reader_ptr: core::ptr::null_mut(),
                            buf_ptr: core::ptr::slice_from_raw_parts_mut(core::ptr::null_mut(), 0),
                            read_buffer: alloc::vec![0; 8192].into_boxed_slice(),
                        })
                    },
                    | Err(e) => return Poll::Ready(Err(e)),
                }
            };

            ctx.status = FiberStatus::Initial;
            ctx.panic_payload = None;
            ctx.result_ptr = core::ptr::null_mut();
            ctx.reader_ptr = core::ptr::null_mut();
            ctx.buf_ptr = core::ptr::slice_from_raw_parts_mut(core::ptr::null_mut(), 0);

            let sp = ctx.stack.top();

            #[cfg(all(target_arch = "x86_64", unix))]
            {
                // x86_64 SysV: RSP must be 16-aligned on entry + 8 (for return address).
                ctx.regs.gprs[0] = sp - 8;
                ctx.regs.gprs[1] = 0; // RBP (frame pointer) = NULL → end of frame chain
                ctx.regs.gprs[7] = fiber_trampoline as *const () as u64; // return address
                // MXCSR default: 0x1F80 = all FP exceptions masked.
                ctx.regs.gprs[8] = 0x1F80;
            }
            #[cfg(all(target_arch = "x86_64", windows))]
            {
                // x86_64 Win64: MUST have 32 bytes of shadow space above the return address.
                // sp - 8 (return addr) - 32 (shadow) = sp - 40.
                ctx.regs.gprs[0] = sp - 40;
                ctx.regs.gprs[1] = 0; // RBP (frame pointer) = NULL
                ctx.regs.gprs[7] = fiber_trampoline as *const () as u64; // return addr
                ctx.regs.gprs[10] = ctx.stack.top(); // TIB StackBase
                ctx.regs.gprs[11] = ctx.stack.bottom(); // TIB StackLimit
                ctx.regs.gprs[12] = ctx.stack.allocation_base(); // TIB DeallocationStack
                ctx.regs.gprs[13] = 0xFFFF_FFFF_FFFF_FFFF_u64; // ExceptionList terminator
                ctx.regs.gprs[14] = 0x1F80;
            }
            #[cfg(all(target_arch = "aarch64", unix))]
            {
                ctx.regs.gprs[10] = 0; // x29 (frame pointer) = NULL
                ctx.regs.gprs[11] = fiber_trampoline as u64; // x30 (LR)
                ctx.regs.gprs[12] = sp; // SP
            }
            #[cfg(all(target_arch = "aarch64", windows))]
            {
                ctx.regs.gprs[10] = 0; // x29 (frame pointer) = NULL
                ctx.regs.gprs[11] = fiber_trampoline as u64; // x30 (LR)
                ctx.regs.gprs[12] = sp; // SP
                ctx.regs.gprs[13] = ctx.stack.top(); // TEB StackBase
                ctx.regs.gprs[14] = ctx.stack.bottom(); // TEB StackLimit
                ctx.regs.gprs[15] = ctx.stack.bottom(); // TEB DeallocationStack
            }
            #[cfg(target_arch = "riscv64")]
            {
                ctx.regs.gprs[0] = sp; // SP
                ctx.regs.gprs[1] = 0; // s0/fp (frame pointer) = NULL
                ctx.regs.gprs[13] = fiber_trampoline as u64; // RA
            }

            let this = unsafe { self.as_mut().get_unchecked_mut() };
            this.ctx = Some(ctx);
        }

        let this = unsafe { self.get_unchecked_mut() };
        let this_ptr = core::ptr::from_mut::<Self>(this).cast::<()>();
        let ctx = this.ctx.as_mut().unwrap();

        // Refresh the result pointer on every poll — the BridgeFuture may
        // have been moved by the executor (it implements Unpin implicitly
        // via DerefMut on the Pin, but we use get_unchecked_mut above).
        ctx.result_ptr = (&raw mut this.result).cast::<()>();

        // -- First poll: set up the closure and do the initial switch --------
        if this.f.is_some() && ctx.status == FiberStatus::Initial {
            unsafe fn invoke<R: futures_io::AsyncRead + Unpin, F, T>(data: *mut ())
            where
                F: FnOnce(&mut FiberReader<'_, R>) -> Result<T, crate::error::DecodeError>,
            {
                unsafe {
                    let this = &mut *data.cast::<BridgeFuture<R, F, T>>();
                    let f = this.f.take().unwrap();
                    let ctx_ptr = CURRENT_FIBER.with(core::cell::Cell::get);
                    let mut real_reader: FiberReader<'_, R> = FiberReader {
                        inner: core::marker::PhantomData,
                        ctx: ctx_ptr,
                    };
                    let res = f(&mut real_reader);
                    let rp = (*ctx_ptr)
                        .result_ptr
                        .cast::<Option<Result<T, crate::error::DecodeError>>>();
                    *rp = Some(res);
                }
            }

            ctx.closure_ptr = this_ptr;
            ctx.invoke_closure = invoke::<R, F, T>;

            ctx.status = FiberStatus::Running;

            unsafe {
                resume_fiber(ctx);
            }
        }

        loop {
            let ctx = this.ctx.as_mut().unwrap();

            match ctx.status {
                | FiberStatus::Finished => {
                    // Return context to pool.
                    if let Some(ctx) = this.ctx.take() {
                        CONTEXT_POOL.with(|pool| {
                            let mut p = pool.borrow_mut();
                            if p.len() < MAX_POOLED_CONTEXTS {
                                p.push(ctx);
                            }
                        });
                    }
                    return Poll::Ready(this.result.take().unwrap());
                },
                | FiberStatus::Panicked => {
                    let payload = ctx.panic_payload.take().unwrap();
                    if let Some(ctx) = this.ctx.take() {
                        CONTEXT_POOL.with(|pool| {
                            let mut p = pool.borrow_mut();
                            if p.len() < MAX_POOLED_CONTEXTS {
                                p.push(ctx);
                            }
                        });
                    }
                    std::panic::resume_unwind(payload);
                },
                | FiberStatus::Yielded => {
                    // The fiber needs more data — try to read from the
                    // async reader.

                    // We must borrow ctx and this.reader disjointly.
                    // Since this.reader and this.ctx are distinct fields, we can do:
                    let ctx_read_buf = &mut ctx.read_buffer[..];
                    let poll_res = Pin::new(&mut this.reader).poll_read(cx, ctx_read_buf);
                    match poll_res {
                        | Poll::Ready(Ok(filled)) => {
                            if filled == 0 {
                                // EOF — tell the fiber so it can return an error.
                                ctx.status = FiberStatus::Finished;
                                ctx.buf_ptr = core::ptr::slice_from_raw_parts_mut(
                                    ctx.read_buffer.as_mut_ptr(),
                                    0,
                                );
                                unsafe {
                                    resume_fiber(ctx);
                                }
                                continue;
                            }
                            ctx.status = FiberStatus::Running;
                            ctx.buf_ptr = core::ptr::slice_from_raw_parts_mut(
                                ctx.read_buffer.as_mut_ptr(),
                                filled,
                            );
                            unsafe {
                                resume_fiber(ctx);
                            }
                        },
                        | Poll::Ready(Err(e)) => {
                            if let Some(ctx) = this.ctx.take() {
                                CONTEXT_POOL.with(|pool| {
                                    let mut p = pool.borrow_mut();
                                    if p.len() < MAX_POOLED_CONTEXTS {
                                        p.push(ctx);
                                    }
                                });
                            }
                            return Poll::Ready(crate::error::cold_decode_error_io(e, 1));
                        },
                        | Poll::Pending => return Poll::Pending,
                    }
                },
                | _ => {
                    unreachable!("invalid fiber status in poll loop");
                },
            }
        }
    }
}

// Clean up fiber resources if the future is dropped while the fiber is
// suspended (e.g. the task is cancelled). The GuardedStack and Context
// memories will be successfully unmapped / dropped natively.
#[cfg(feature = "async-fiber")]
impl<R, F, T> Drop for BridgeFuture<R, F, T> {
    #[inline(always)]
    fn drop(&mut self) {
        if let Some(ctx) = self.ctx.take() {
            // Context is dropped instead of pooled to discard dirty internal state.
            drop(ctx);
        }
    }
}

/// A lightweight adapter to use `tokio::io::AsyncRead` with `AsyncFiberBridge`.
#[cfg(all(feature = "tokio", feature = "async-fiber"))]
pub(crate) struct TokioReader<R>(pub(crate) R);

#[cfg(all(feature = "tokio", feature = "async-fiber"))]
impl<R: tokio::io::AsyncRead + Unpin> futures_io::AsyncRead for TokioReader<R> {
    #[inline]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<std::io::Result<usize>> {
        let mut read_buf = tokio::io::ReadBuf::new(buf);
        match Pin::new(&mut self.0).poll_read(cx, &mut read_buf) {
            | Poll::Ready(Ok(())) => Poll::Ready(Ok(read_buf.filled().len())),
            | Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            | Poll::Pending => Poll::Pending,
        }
    }
}
