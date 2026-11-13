//! A minimal inline hook. The target's first instructions are replaced by an absolute jump to the
//! hook, and a trampoline runs the replaced instructions and jumps back. The caller supplies the
//! exact bytes it expects there, which must be whole instructions without RIP-relative operands.

use std::{ffi::c_void, ptr, thread, time::Duration};

use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        Diagnostics::{
            Debug::{CONTEXT, CONTEXT_CONTROL_AMD64, FlushInstructionCache, GetThreadContext},
            ToolHelp::{CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next},
        },
        Memory::{
            MEM_COMMIT, MEM_RESERVE, PAGE_EXECUTE_READ, PAGE_EXECUTE_READWRITE, PAGE_PROTECTION_FLAGS, PAGE_READWRITE,
            VirtualAlloc, VirtualProtect,
        },
        Threading::{
            GetCurrentProcess, GetCurrentProcessId, GetCurrentThreadId, OpenThread, ResumeThread,
            SuspendThread, THREAD_GET_CONTEXT, THREAD_SUSPEND_RESUME,
        },
    },
};

/// `jmp qword ptr [rip+0]` followed by the 8-byte destination.
const ABS_JUMP_LEN: usize = 14;

#[derive(Debug)]
pub enum Error {
    /// The bytes at the target aren't the expected ones. Holds what was found, for the log.
    Mismatch(Vec<u8>),
    Alloc,
    Protect,
    /// A thread kept stopping inside the bytes being replaced.
    Busy,
}

/// Hooks `target`. Returns the address to call to run the original function.
///
/// # Safety
/// `target` must be the start of a function whose first `expected.len()` bytes are whole
/// instructions that don't depend on their address, and `hook` must have the same signature.
pub unsafe fn install(target: usize, expected: &[u8], hook: usize) -> Result<usize, Error> {
    let len = expected.len();
    assert!(len >= ABS_JUMP_LEN);
    // SAFETY: the target lies in the game's loaded image, which is readable.
    let found = unsafe { std::slice::from_raw_parts(target as *const u8, len) };
    if found != expected {
        return Err(Error::Mismatch(found.to_vec()));
    }

    // SAFETY: plain allocation of new memory.
    let trampoline =
        unsafe { VirtualAlloc(None, 64, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE) } as *mut u8;
    if trampoline.is_null() {
        return Err(Error::Alloc);
    }
    let mut code = expected.to_vec();
    code.extend_from_slice(&abs_jump(target + len));
    // SAFETY: the allocation is 64 bytes, which fits the copied bytes and the jump back.
    unsafe { ptr::copy_nonoverlapping(code.as_ptr(), trampoline, code.len()) };

    let mut patch = abs_jump(hook).to_vec();
    patch.resize(len, 0x90);

    // Everything that allocates happens before the other threads are paused, as one of them could
    // hold the heap lock.
    let mut threads = Vec::with_capacity(512);
    for _ in 0..50 {
        threads.clear();
        // SAFETY: the list has room reserved, and the patch is the right length.
        match unsafe { patch_with_threads_paused(target, &patch, &mut threads) } {
            Ok(true) => return Ok(trampoline as usize),
            Ok(false) => thread::sleep(Duration::from_millis(5)),
            Err(e) => return Err(e),
        }
    }
    Err(Error::Busy)
}

fn abs_jump(to: usize) -> [u8; ABS_JUMP_LEN] {
    let mut jump = [0xFF, 0x25, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    jump[6..].copy_from_slice(&(to as u64).to_le_bytes());
    jump
}

/// Returns false, having changed nothing, if a thread was stopped inside the bytes to replace.
unsafe fn patch_with_threads_paused(
    target: usize,
    patch: &[u8],
    threads: &mut Vec<HANDLE>,
) -> Result<bool, Error> {
    // SAFETY: reads the thread list and pauses the process's own threads, which are all resumed
    // below on every path.
    unsafe {
        suspend_others(threads);
        let mut context = Context::default();
        let mut safe = true;
        for &thread in threads.iter() {
            context.0.ContextFlags = CONTEXT_CONTROL_AMD64;
            if GetThreadContext(thread, &mut context.0).is_ok() {
                let rip = context.0.Rip as usize;
                if (target..target + patch.len()).contains(&rip) {
                    safe = false;
                    break;
                }
            }
        }
        let mut result = Ok(safe);
        if safe {
            let mut old = PAGE_PROTECTION_FLAGS(0);
            if VirtualProtect(target as *const c_void, patch.len(), PAGE_EXECUTE_READWRITE, &mut old).is_err() {
                result = Err(Error::Protect);
            } else {
                ptr::copy_nonoverlapping(patch.as_ptr(), target as *mut u8, patch.len());
                let restore = if old.0 == 0 { PAGE_EXECUTE_READ } else { old };
                let _ = VirtualProtect(target as *const c_void, patch.len(), restore, &mut old);
                let _ = FlushInstructionCache(GetCurrentProcess(), Some(target as *const c_void), patch.len());
            }
        }
        for &thread in threads.iter() {
            ResumeThread(thread);
            let _ = CloseHandle(thread);
        }
        result
    }
}

/// `CONTEXT` must be 16-byte aligned for `GetThreadContext`.
#[repr(C, align(16))]
#[derive(Default)]
struct Context(CONTEXT);

/// Pauses every other thread of this process, up to the list's reserved capacity.
unsafe fn suspend_others(threads: &mut Vec<HANDLE>) {
    // SAFETY: Toolhelp calls on a snapshot this function owns and closes.
    unsafe {
        let Ok(snapshot) = CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) else { return };
        let process = GetCurrentProcessId();
        let me = GetCurrentThreadId();
        let mut entry = THREADENTRY32 { dwSize: size_of::<THREADENTRY32>() as u32, ..Default::default() };
        let mut more = Thread32First(snapshot, &mut entry).is_ok();
        while more && threads.len() < threads.capacity() {
            if entry.th32OwnerProcessID == process && entry.th32ThreadID != me {
                if let Ok(thread) =
                    OpenThread(THREAD_SUSPEND_RESUME | THREAD_GET_CONTEXT, false, entry.th32ThreadID)
                {
                    if SuspendThread(thread) != u32::MAX {
                        threads.push(thread);
                    } else {
                        let _ = CloseHandle(thread);
                    }
                }
            }
            more = Thread32Next(snapshot, &mut entry).is_ok();
        }
        let _ = CloseHandle(snapshot);
    }
}

/// Replaces a function pointer, such as a vtable entry, if it holds `expected`. Returns what it held.
///
/// # Safety
/// `slot` must be a pointer-sized, aligned location in the game image, and `new` must have the
/// signature of the function it replaces.
pub unsafe fn swap_pointer(slot: usize, expected: usize, new: usize) -> Result<usize, Error> {
    // SAFETY: the caller guarantees the slot is readable.
    let found = unsafe { *(slot as *const usize) };
    if found != expected {
        return Err(Error::Mismatch(found.to_le_bytes().to_vec()));
    }
    let mut old = PAGE_PROTECTION_FLAGS(0);
    // SAFETY: the slot is in the image; an aligned pointer-sized write is atomic, so a thread
    // reading it sees either the old or the new function.
    unsafe {
        if VirtualProtect(slot as *const c_void, 8, PAGE_READWRITE, &mut old).is_err() {
            return Err(Error::Protect);
        }
        (slot as *mut usize).write_volatile(new);
        let _ = VirtualProtect(slot as *const c_void, 8, old, &mut old);
    }
    Ok(found)
}
