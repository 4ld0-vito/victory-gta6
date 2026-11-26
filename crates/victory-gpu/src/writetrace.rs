//! `VICTORY_WRITE_TRACE=1` (a debugging aid): every write of ours into title
//! memory after a submission ([`crate::host::write_mapped`]) is kept in a
//! ring, and an access violation names the kept writes near the faulting
//! thread's registers: whether title objects the title then trips over were
//! last written by us (what shaders wrote, put back) or not.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

const N: usize = 1 << 20;
static DST: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static LEN: [AtomicU64; N] = [const { AtomicU64::new(0) }; N];
static NEXT: AtomicUsize = AtomicUsize::new(0);
static SAID: AtomicUsize = AtomicUsize::new(0);
static ARMED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// From now on faults are reported (the structure builder runs).
pub(crate) fn arm() {
    ARMED.store(true, Ordering::Relaxed);
}

pub(crate) fn on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on = std::env::var_os("VICTORY_WRITE_TRACE").is_some_and(|v| v == "1");
        if on {
            install();
        }
        on
    })
}

/// Notes a write of `len` bytes at `dst` into title memory.
pub(crate) fn note(dst: usize, len: usize) {
    if !on() {
        return;
    }
    let i = NEXT.fetch_add(1, Ordering::Relaxed) % N;
    let frame = crate::frame::current() & 0xff_ffff;
    DST[i].store(dst as u64, Ordering::Relaxed);
    LEN[i].store((len as u64) | frame << 40, Ordering::Relaxed);
}

/// Our executable's image: base and size.
fn own_image() -> (u64, u64) {
    static IMAGE: std::sync::OnceLock<(u64, u64)> = std::sync::OnceLock::new();
    *IMAGE.get_or_init(|| {
        use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
        // SAFETY: our own module, mapped for the process's life; its headers
        // are readable.
        unsafe {
            let base = GetModuleHandleW(std::ptr::null()) as *const u8;
            let pe = base.add(std::ptr::read_unaligned(base.add(0x3c).cast::<u32>()) as usize);
            let size = std::ptr::read_unaligned(pe.add(24 + 56).cast::<u32>());
            (base as u64, u64::from(size))
        }
    })
}

fn install() {
    own_image();
    use windows_sys::Win32::Foundation::{EXCEPTION_ACCESS_VIOLATION, NTSTATUS};
    use windows_sys::Win32::System::Diagnostics::Debug::{
        AddVectoredExceptionHandler, EXCEPTION_CONTINUE_SEARCH, EXCEPTION_POINTERS,
    };
    unsafe extern "system" fn on_fault(info: *mut EXCEPTION_POINTERS) -> i32 {
        // SAFETY: the system passes the exception being dispatched.
        let (r, c) = unsafe { (&*(*info).ExceptionRecord, &*(*info).ContextRecord) };
        if r.ExceptionCode != EXCEPTION_ACCESS_VIOLATION as NTSTATUS || r.NumberParameters < 2 {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        // Our own code faults on purpose (guards, safe copies): only faults
        // outside our executable.
        let at = r.ExceptionInformation[1] as u64;
        let (base, size) = own_image();
        if (base..base + size).contains(&c.Rip) {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        // Pointer garbage only (near null, or far past title memory), and
        // only once the structure builder runs.
        if !ARMED.load(Ordering::Relaxed) || !(at < 0x1_0000 || at >= 0x200_0000_0000) {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        if SAID.fetch_add(1, Ordering::Relaxed) >= 6 {
            return EXCEPTION_CONTINUE_SEARCH;
        }
        let regs = [
            ("rax", c.Rax), ("rbx", c.Rbx), ("rcx", c.Rcx), ("rdx", c.Rdx), ("rsi", c.Rsi),
            ("rdi", c.Rdi), ("rbp", c.Rbp), ("r8", c.R8), ("r9", c.R9), ("r10", c.R10),
            ("r11", c.R11), ("r12", c.R12), ("r13", c.R13), ("r14", c.R14), ("r15", c.R15),
        ];
        let next = NEXT.load(Ordering::Relaxed);
        let kept = next.min(N);
        eprintln!(
            "WRITE-TRACE: fault at rip {:#x} on {at:#x}, frame {}, {} writes kept of {next}",
            c.Rip,
            crate::frame::current(),
            kept
        );
        for (name, v) in regs {
            if !(0x1_0000..0x100_0000_0000).contains(&v) {
                continue;
            }
            let (lo, hi) = (v.saturating_sub(0x200), v + 0x200);
            let mut found = 0;
            for back in 0..kept {
                let i = (next + N - 1 - back) % N;
                let d = DST[i].load(Ordering::Relaxed);
                let l = LEN[i].load(Ordering::Relaxed);
                let (len, frame) = (l & 0xff_ffff_ffff, l >> 40);
                if d < hi && lo < d + len {
                    eprintln!("WRITE-TRACE:   {name}={v:#x}: written by us at {d:#x}+{len:#x} in frame {frame} ({back} writes ago)");
                    found += 1;
                    if found >= 4 {
                        break;
                    }
                }
            }
            if found == 0 {
                eprintln!("WRITE-TRACE:   {name}={v:#x}: no write of ours within 0x200");
            }
        }
        EXCEPTION_CONTINUE_SEARCH
    }
    // SAFETY: the handler only reads the ring and the context.
    unsafe {
        AddVectoredExceptionHandler(1, Some(on_fault));
    }
}
