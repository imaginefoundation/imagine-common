//! Shell de texto do ImagineOS — interpretador DSH (DotShell).
//!
//! Implementa a especificação em `docs/imagine/idshspec.md`:
//! sintaxe baseada em pontos (`.run("programa")`, `.args(chave="valor")`,
//! `.if()`, `.loop()`, `.scope()`, etc.), variáveis com escopo, built-ins e
//! execução de programas de userspace via syscalls do kernel DNU.
//!
//! Roda inteiramente em userspace (Ring 3). Não há código de shell no kernel.

#![no_std]
#![no_main]
#![allow(unsafe_op_in_unsafe_fn)]

mod lexer;
mod parser;
mod exec;
mod env;
mod line;
mod prompt;
mod history;
mod uri;
mod complete;
mod highlight;

use core::panic::PanicInfo;

/// Syscalls do kernel DNU (contrato em dnu/abi/src/syscall.rs).
const SYS_WRITE: u64 = 1;
const SYS_READ: u64 = 2;
const SYS_YIELD: u64 = 3;
const SYS_EXIT: u64 = 4;
const SYS_GETPID: u64 = 5;
const SYS_EXECVE: u64 = 47;
const SYS_WAITPID: u64 = 46;
const SYS_FORK: u64 = 45;
const SYS_PIPE: u64 = 42;
const SYS_OPEN: u64 = 17;
const SYS_READ_FD: u64 = 18;
const SYS_WRITE_FD: u64 = 19;
const SYS_CLOSE: u64 = 20;
const SYS_ISFILE: u64 = 9;
const SYS_ISDIR: u64 = 8;
const SYS_READDIR: u64 = 11;

/// Escreve bytes no console do kernel.
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

/// Lê um caractere do console do kernel. Devolve None se não houver.
fn console_read_char() -> Option<u8> {
    let mut byte: u8 = 0;
    let ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_READ,
            in("rdi") &mut byte as *mut u8 as u64,
            in("rsi") 1usize,
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    if ret > 0 { Some(byte) } else { None }
}

/// Ponto de entrada do shell (sem main, como todo programa bare-metal).
#[unsafe(no_mangle)]
extern "C" fn _start(_argc: usize, _argv: *const *const u8) -> ! {
    console_write(b"\r\n=== ImagineOS DSH (DotShell) ===\r\n");
    console_write(b"Digite um comando DSH ou 'help' para listar built-ins.\r\n\r\n");

    let mut environment = env::Environment::new();
    let mut cwd: [u8; uri::MAX_PATH] = [0; uri::MAX_PATH];
    let mut cwd_length = 1usize;
    cwd[0] = b'/';
    let history = history::History::new();

    let mut editor = line::Editor::new();
    let mut line_buf: [u8; line::MAX_LINE] = [0; line::MAX_LINE];

    loop {
        prompt::write_prompt(&cwd[..cwd_length]);

        let mut length = 0usize;
        match editor.read_line(&mut line_buf, &cwd[..cwd_length], &mut length) {
            line::LineResult::Line => {
                let line = &line_buf[..length];
                if line.is_empty() {
                    continue;
                }
                // Executa a linha como DSH.
                let mut status = exec::run_line(
                    line,
                    &mut environment,
                    &mut cwd,
                    &mut cwd_length,
                    &history,
                );
                environment.set_status(i64::from(status));
                if status != 0 {
                    let mut digits = [0u8; 16];
                    let n = fmt_decimal(status as u64, &mut digits);
                    console_write(b"\r\n[status ");
                    console_write(&digits[..n]);
                    console_write(b"]\r\n");
                }
            }
            line::LineResult::Interrupted => {
                console_write(b"^C\r\n");
            }
            line::LineResult::Exit => {
                console_write(b"\r\nsaindo do shell...\r\n");
                break;
            }
        }
        let _ = unsafe { core::arch::asm!("yield", options(nostack, nomem)) };
    }

    console_write(b"\r\nshell finalizado.\r\n");
    unsafe { core::arch::asm!("syscall", in("rax") SYS_EXIT, in("rdi") 0u64, options(noreturn, nostack, nomem)) };
}

/// Formata um inteiro decimal sem sinal em um buffer.
fn fmt_decimal(mut value: u64, output: &mut [u8]) -> usize {
    if value == 0 {
        output[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 20];
    let mut cursor = digits.len();
    while value != 0 {
        cursor -= 1;
        digits[cursor] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    let length = digits.len() - cursor;
    output[..length].copy_from_slice(&digits[cursor..]);
    length
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    console_write(b"\r\nDSH PANIC\r\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}
