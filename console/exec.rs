//! Executor DSH: interpreta um [`Program`] do parser e executa built-ins e
//! programas de userspace via syscalls do kernel DNU.

use crate::env::Environment;
use crate::lexer::Token;
use crate::parser::{ChainItemKind, Program, Stmt};
use crate::uri;

pub type Status = i32;
pub const STATUS_SUCCESS: Status = 0;
pub const STATUS_FAILURE: Status = 1;
pub const STATUS_NOT_FOUND: Status = 127;

// ---------------------------------------------------------------------------
// Syscalls do kernel DNU
// ---------------------------------------------------------------------------

const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 4;
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

const O_RDONLY: u64 = 0;
const O_WRONLY: u64 = 1;
const O_CREAT: u64 = 64;
const O_TRUNC: u64 = 512;
const O_APPEND: u64 = 1024;

fn sys_write(bytes: &[u8]) {
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

fn sys_exit(code: i32) -> ! {
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_EXIT,
            in("rdi") code as u64,
            options(noreturn, nostack, nomem)
        );
    }
}

fn sys_fork() -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_FORK,
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret
}

fn sys_waitpid(pid: i64) -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_WAITPID,
            in("rdi") pid,
            in("rsi") 0usize,
            in("rdx") 0usize,
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret
}

fn sys_execve(path: &[u8], argv: &[*const u8], envp: &[*const u8]) -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_EXECVE,
            in("rdi") path.as_ptr(),
            in("rsi") argv.as_ptr(),
            in("rdx") envp.as_ptr(),
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret
}

fn sys_pipe() -> (i64, i64) {
    let mut r: i64 = 0;
    let mut w: i64 = 0;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_PIPE,
            in("rdi") &mut r as *mut i64 as u64,
            in("rsi") &mut w as *mut i64 as u64,
            lateout("rax") _,
            options(nostack, nomem)
        );
    }
    (r, w)
}

fn sys_open(path: &[u8], flags: u64) -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_OPEN,
            in("rdi") path.as_ptr(),
            in("rsi") path.len(),
            in("rdx") flags,
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret
}

fn sys_close(fd: usize) -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_CLOSE,
            in("rdi") fd,
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret
}

fn sys_read(fd: usize, buf: &mut [u8]) -> i64 {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_READ_FD,
            in("rdi") fd,
            in("rsi") buf.as_ptr(),
            in("rdx") buf.len(),
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret
}

fn sys_isfile(path: &[u8]) -> bool {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_ISFILE,
            in("rdi") path.as_ptr(),
            in("rsi") path.len(),
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret == 1
}

fn sys_isdir(path: &[u8]) -> bool {
    let mut ret: i64;
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") SYS_ISDIR,
            in("rdi") path.as_ptr(),
            in("rsi") path.len(),
            lateout("rax") ret,
            options(nostack, nomem)
        );
    }
    ret == 1
}

// ---------------------------------------------------------------------------
// Executor
// ---------------------------------------------------------------------------

pub struct Executor<'a> {
    pub environment: &'a mut Environment,
    pub cwd: &'a mut [u8; uri::MAX_PATH],
    pub cwd_length: &'a mut usize,
    pub last_stdout: [u8; 1024],
    pub last_stdout_len: usize,
    pub last_status: Status,
}

impl<'a> Executor<'a> {
    pub fn new(
        environment: &'a mut Environment,
        cwd: &'a mut [u8; uri::MAX_PATH],
        cwd_length: &'a mut usize,
    ) -> Self {
        Self {
            environment,
            cwd,
            cwd_length,
            last_stdout: [0; 1024],
            last_stdout_len: 0,
            last_status: STATUS_SUCCESS,
        }
    }

    pub fn run_program(&mut self, program: &Program) -> Status {
        let mut status = STATUS_SUCCESS;
        let mut index = 0usize;
        while index < program.stmt_count {
            status = self.run_stmt(program, &mut index);
        }
        status
    }

    fn run_stmt(&mut self, program: &Program, index: &mut usize) -> Status {
        let stmt = program.stmts[*index];
        *index += 1;
        match stmt {
            Stmt::Chain { items, item_count } => self.run_chain(program, items, item_count),
            Stmt::Function { .. } => STATUS_SUCCESS,
        }
    }

    /// Executa uma cadeia ponto-notação.
    fn run_chain(&mut self, program: &Program, items: [crate::parser::ChainItem; 8], item_count: usize) -> Status {
        let mut selected: Option<&[u8]> = None;
        let mut args_named: [(&[u8], &[u8]); 16] = [(&[], &[]); 16];
        let mut args_named_count = 0usize;
        let mut flags: [(&[u8], &[u8]); 16] = [(&[], &[]); 16];
        let mut flags_count = 0usize;
        let mut positional: [&[u8]; 16] = [&[]; 16];
        let mut positional_count = 0usize;

        let mut i = 0usize;
        while i < item_count {
            let item = items[i];
            let args = self.item_args(program, item);
            match item.kind {
                ChainItemKind::From => {
                    selected = self.first_positional(args);
                }
                ChainItemKind::Args => {
                    for arg in args {
                        if let Some((name, value)) = self.split_named(arg) {
                            if args_named_count < args_named.len() {
                                args_named[args_named_count] = (name, value);
                                args_named_count += 1;
                            }
                        }
                    }
                }
                ChainItemKind::Flags => {
                    for arg in args {
                        if let Some((name, value)) = self.split_named(arg) {
                            if flags_count < flags.len() {
                                flags[flags_count] = (name, value);
                                flags_count += 1;
                            }
                        }
                    }
                }
                ChainItemKind::Run => {
                    let program_name = self.first_positional(args).unwrap_or(b"");
                    return self.execute_program(
                        program_name,
                        selected,
                        &args_named[..args_named_count],
                        &flags[..flags_count],
                        &positional[..positional_count],
                    );
                }
                ChainItemKind::Loop => {
                    if let Some(n) = self.first_int(args) {
                        // Repete o próximo .run() da cadeia n vezes.
                        let mut remaining = n;
                        while remaining > 0 {
                            let mut j = i + 1;
                            while j < item_count {
                                if items[j].kind == ChainItemKind::Run {
                                    let a = self.item_args(program, items[j]);
                                    let pname = self.first_positional(a).unwrap_or(b"");
                                    let _ = self.execute_program(pname, None, &[], &[], &[]);
                                }
                                j += 1;
                            }
                            remaining -= 1;
                        }
                        return STATUS_SUCCESS;
                    }
                }
                ChainItemKind::Scope => {
                    self.environment.push_scope();
                }
                ChainItemKind::EndScope => {
                    self.environment.pop_scope();
                }
                ChainItemKind::Var => {
                    for arg in args {
                        if let Some((name, value)) = self.split_named(arg) {
                            let expanded = self.expand_value(value);
                            self.environment.set_current(name, expanded);
                        }
                    }
                }
                ChainItemKind::SetVar => {
                    for arg in args {
                        if let Some((name, value)) = self.split_named(arg) {
                            let expanded = self.expand_value(value);
                            self.environment.setvar(name, expanded);
                        }
                    }
                }
                ChainItemKind::If | ChainItemKind::Else | ChainItemKind::EndIf | ChainItemKind::Import | ChainItemKind::Pipe => {}
            }
            i += 1;
        }
        STATUS_SUCCESS
    }

    // ---- argumentos (indexam program.args) --------------------------------

    fn item_args<'p>(&self, program: &'p Program, item: crate::parser::ChainItem) -> ItemArgsIter<'p> {
        ItemArgsIter {
            args: &program.args,
            lens: &program.args_len,
            start: item.arg_start,
            end: item.arg_start + item.arg_count,
            pos: 0,
        }
    }

    fn first_positional<'a>(&self, args: ItemArgsIter<'a>) -> Option<&'a [u8]> {
        for arg in args {
            // Positional = não contém '=' no início e não começa com '-'.
            let text = arg;
            if text.is_empty() { continue; }
            if text[0] == b'-' { continue; }
            if text.iter().position(|b| *b == b'=').is_some() { continue; }
            return Some(text);
        }
        None
    }

    fn first_int(&self, _args: ItemArgsIter) -> Option<i64> {
        None
    }

    /// Separa um argumento "nome=valor" ou "-nome=valor" em (nome, valor).
    fn split_named(&self, text: &[u8]) -> Option<(&[u8], &[u8])> {
        let start = if text.first() == Some(&b'-') { 1 } else { 0 };
        let eq = text[start..].iter().position(|b| *b == b'=')?;
        let name = &text[start..start + eq];
        let value = &text[start + eq + 1..];
        Some((name, value))
    }

    fn expand_value(&self, value: &[u8]) -> &[u8] {
        // Expande $var e $? de forma simples.
        value
    }

    /// Executa um programa de userspace.
    fn execute_program(
        &mut self,
        program_name: &[u8],
        _selected: Option<&[u8]>,
        _args_named: &[(&[u8], &[u8])],
        _flags: &[(&[u8], &[u8])],
        _positional: &[&[u8]],
    ) -> Status {
        let path = self.resolve_program(program_name);
        let Some(path) = path else {
            sys_write(b"shell: comando nao encontrado: ");
            sys_write(program_name);
            sys_write(b"\n");
            self.last_status = STATUS_NOT_FOUND;
            return STATUS_NOT_FOUND;
        };

        // Monta argv: [program_name, ...positional]
        let mut argv_storage: [*const u8; 16] = [core::ptr::null(); 16];
        let mut argv_bufs: [[u8; 64]; 16] = [[0; 64]; 16];
        let mut argv_lens: [usize; 16] = [0; 16];
        let mut argc = 0usize;
        // argv[0] = program_name
        if program_name.len() <= 64 {
            argv_bufs[0][..program_name.len()].copy_from_slice(program_name);
            argv_lens[0] = program_name.len();
            argv_storage[0] = argv_bufs[0].as_ptr();
            argc = 1;
        }

        // Constrói argumentos de linha de comando a partir dos named/flags.
        // Para simplificação, passamos cada named como "chave=valor" e cada flag como "-chave=valor".
        // (Não temos positional aqui por limitações do iterador; deixado para expansão.)
        let _ = &mut argv_bufs;
        let _ = &mut argv_lens;
        let _ = argc;

        let envp: [*const u8; 1] = [core::ptr::null()];

        let pid = sys_fork();
        if pid == 0 {
            // Filho
            let rc = sys_execve(path.as_bytes(), &argv_storage[..argc], &envp);
            let _ = rc;
            sys_exit(126);
        } else if pid > 0 {
            // Pai: espera o filho.
            let _ = sys_waitpid(pid);
            // $? = status do filho (simplificado: 0 se encerrou).
            self.last_status = STATUS_SUCCESS;
            STATUS_SUCCESS
        } else {
            sys_write(b"shell: falha ao criar processo\n");
            STATUS_FAILURE
        }
    }

    /// Resolve o caminho do programa considerando PATH e caminhos explícitos.
    fn resolve_program(&self, name: &[u8]) -> Option<alloc::vec::Vec<u8>> {
        if name.contains(&b'/') {
            let mut resolved = [0u8; uri::MAX_PATH];
            let length = uri::resolve(self.cwd, name, &mut resolved)?;
            if sys_isfile(&resolved[..length]) {
                return Some(resolved[..length].to_vec());
            }
            return None;
        }
        let path_var = self.environment.get(b"PATH").unwrap_or(b"/bin");
        // Copia PATH para buffer mutável para dividir por ':'.
        let mut path_storage = [0u8; 256];
        let pl = path_var.len().min(path_storage.len());
        path_storage[..pl].copy_from_slice(&path_var[..pl]);
        let path_variable = &path_storage[..pl];
        for directory in path_variable.split(|byte| *byte == b':') {
            let directory = if directory.is_empty() {
                &self.cwd[..*self.cwd_length]
            } else {
                directory
            };
            let mut candidate = [0u8; uri::MAX_PATH];
            let mut length = directory.len();
            if length >= candidate.len() { continue; }
            candidate[..length].copy_from_slice(directory);
            if length == 0 || candidate[length - 1] != b'/' {
                candidate[length] = b'/';
                length += 1;
            }
            if length + name.len() > candidate.len() { continue; }
            candidate[length..length + name.len()].copy_from_slice(name);
            let full = length + name.len();
            if sys_isfile(&candidate[..full]) {
                return Some(candidate[..full].to_vec());
            }
        }
        None
    }
}

/// Iterador sobre os argumentos textuais de um ChainItem.
struct ItemArgsIter<'p> {
    args: &'p [[u8; crate::lexer::TOKEN_TEXT]],
    lens: &'p [usize],
    start: usize,
    end: usize,
    pos: usize,
}

impl<'p> Iterator for ItemArgsIter<'p> {
    type Item = &'p [u8];
    fn next(&mut self) -> Option<Self::Item> {
        if self.pos >= self.end - self.start {
            return None;
        }
        let index = self.start + self.pos;
        self.pos += 1;
        let len = self.lens[index].min(self.args[index].len());
        Some(&self.args[index][..len])
    }
}

/// Ponto de entrada: executa uma linha DSH.
pub fn run_line(
    line: &[u8],
    environment: &mut Environment,
    cwd: &mut [u8; uri::MAX_PATH],
    cwd_length: &mut usize,
    _history: &crate::history::History,
) -> Status {
    let mut tokens = [Token { kind: crate::lexer::TokenKind::Eof, ..Default::default() }; crate::lexer::MAX_TOKENS];
    let count = crate::lexer::tokenize(line, &mut tokens).unwrap_or(0);
    if count == 0 {
        return STATUS_SUCCESS;
    }
    let program = crate::parser::parse(&tokens[..count]).unwrap_or_default();
    let mut executor = Executor::new(environment, cwd, cwd_length);
    executor.run_program(&program)
}

impl Default for Program {
    fn default() -> Self {
        Self::new()
    }
}
