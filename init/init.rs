//! /sbin/init — Primeiro processo (PID 1) do ImagineOS.

#![no_std]
#![no_main]

const SYS_EXEC: u64 = 7;
const SYS_WAITPID: u64 = 46;
const SYS_YIELD: u64 = 3;
const SYS_WRITE: u64 = 1;

fn console_write(bytes: &[u8]) {
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WRITE,
            in("rdi") bytes.as_ptr(),
            in("rsi") bytes.len(),
            in("r10") 0usize,
            in("r8") 0usize,
            in("r9") 0usize,
            lateout("rax") _,
            options(nostack)
        );
    }
}

fn spawn(path: &[u8]) -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_EXEC,
            in("rdi") path.as_ptr(),
            in("rsi") path.len(),
            in("rdx") 0usize,
            in("r10") 0usize,
            in("r8") 0usize,
            in("r9") 0usize,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

fn waitpid() -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WAITPID,
            in("rdi") -1i64,
            in("rsi") 0usize,
            in("rdx") 0usize,
            in("r10") 0usize,
            in("r8") 0usize,
            in("r9") 0usize,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

fn yield_syscall() -> u64 {
    let mut ret: u64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_YIELD,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    console_write(b"\r\n=== init PID 1 ===\r\n");
    console_write(b"spawning /bin/ws_server...\r\n");
    let _ = spawn(b"/bin/ws_server");
    console_write(b"init: entering yield loop\r\n");
    loop {
        let _ = yield_syscall();
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)); }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    console_write(b"\r\ninit PANIC\r\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}
