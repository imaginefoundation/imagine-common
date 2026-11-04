//! /bin/explorer — Shell gráfico mínimo do ImagineOS.
//! Versão simplificada para teste de boot.

#![no_std]
#![no_main]

const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 4;

fn console_write(bytes: &[u8]) {
    unsafe {
        core::arch::asm!(
            "int 0x80",
            in("rax") SYS_WRITE,
            in("rdi") bytes.as_ptr(),
            in("rsi") bytes.len(),
            options(nostack, nomem)
        );
    }
}

#[unsafe(no_mangle)]
extern "C" fn _start(_argc: usize, _argv: *const *const u8) -> ! {
    console_write(b"\r\n=== ImagineOS Explorer (Shell Grafico) ===\r\n");
    console_write(b"Taskbar, area de trabalho e menu Iniciar.\r\n");
    console_write(b"Rodando em modo usuariosapce (Ring 3).\r\n");
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
