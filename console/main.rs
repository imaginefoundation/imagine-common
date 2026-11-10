//! Shell de linha de comando do ImagineOS (versão mínima para teste).

#![no_std]
#![no_main]

use dnu_abi::{SYS_WRITE, SYS_EXIT};

fn console_write(bytes: &[u8]) {
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WRITE,
            in("rdi") bytes.as_ptr(),
            in("rsi") bytes.len(),
            options(nostack, nomem)
        );
    }
}

#[unsafe(no_mangle)]
extern "C" fn _start(_argc: usize, _argv: *const *const u8) -> ! {
    console_write(b"\r\n=== ImagineOS Console ===\r\n");
    console_write(b"Shell minima em modo usuariosapce.\r\n");
    console_write(b"Aguardando...\r\n");

    loop {
        unsafe { core::arch::asm!("hlt", options(nomem, nostack)); }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    console_write(b"\r\nPANIC\r\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}
