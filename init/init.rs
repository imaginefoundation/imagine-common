//! /sbin/init — Primeiro processo (PID 1) do ImagineOS.
//! Lança ws_server, shell e explorer via syscall.

#![no_std]
#![no_main]

const SYS_EXEC: u64 = 7;
const SYS_WAITPID: u64 = 46;

/// Faz spawn de um programa (SYS_EXEC) via syscall.
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
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

/// Espera por qualquer filho (SYS_WAITPID).
fn waitpid() -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WAITPID,
            in("rdi") -1i64,
            in("rsi") 0usize,
            in("rdx") 0usize,
            lateout("rax") ret,
            options(nostack)
        );
    }
    ret
}

/// Ponto de entrada do init (PID 1).
#[unsafe(no_mangle)]
extern "C" fn _start() -> ! {
    // Lança o servidor de janelas.
    let _ = spawn(b"/bin/ws_server");

    // Lança o shell.
    let _ = spawn(b"/bin/shell");

    // Lança o shell gráfico.
    let _ = spawn(b"/bin/explorer");

    // Aguarda os filhos terminarem.
    loop {
        let _ = waitpid();
        unsafe { core::arch::asm!("hlt", options(nostack)); }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop { unsafe { core::arch::asm!("hlt", options(nostack)); } }
}