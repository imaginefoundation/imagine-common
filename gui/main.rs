//! /bin/ws_server — Servidor de janelas em modo usuário.
//! Executa em Ring 3 como um servidor de janelas e compositor em userspace.

#![no_std]
#![no_main]

mod gdi;
mod wm;

use gdi::{HDC, Rect, COLOR_3DFACE, draw_cursor};
use wm::WindowManager;

const SYS_FB_INFO: u64 = 48;
const SYS_FB_MAP: u64 = 49;
const SYS_INPUT_POLL: u64 = 50;
const SYS_CONSOLE_RELEASE: u64 = 51;
const SYS_POINTER_POSITION: u64 = 52;
const SYS_SURFACE_CREATE: u64 = 53;
const SYS_SURFACE_PRESENT: u64 = 54;
const SYS_SURFACE_FLUSH: u64 = 55;
const SYS_SURFACE_DESTROY: u64 = 56;
const SYS_SURFACE_POLL: u64 = 57;
const SYS_SURFACE_SNAPSHOT: u64 = 58;
const SYS_SURFACE_SEND_EVENT: u64 = 59;
const SYS_SURFACE_MOVE: u64 = 61;
const SYS_WRITE: u64 = 1;
const SYS_EXIT: u64 = 4;

const INPUT_KEY: u32 = 1;
const INPUT_MOUSE_MOVE: u32 = 2;
const INPUT_MOUSE_BUTTON: u32 = 3;

const WM_CREATE: u32 = 0x0001;
const WM_DESTROY: u32 = 0x0002;
const WM_PAINT: u32 = 0x000F;
const WM_CLOSE: u32 = 0x0010;
const WM_QUIT: u32 = 0x0012;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_KEYDOWN: u32 = 0x0100;
const WM_NCCREATE: u32 = 0x0081;
const WM_NCCALCSIZE: u32 = 0x0083;

const MAX_WINDOW_PROCS: usize = 16;

static mut WINDOW_PROCS: [core::cell::UnsafeCell<Option<extern "C" fn(u32, u32, usize, isize) -> isize>>; MAX_WINDOW_PROCS] =
    unsafe { core::mem::zeroed() };

fn register_wndproc(hwnd: u32, proc: extern "C" fn(u32, u32, usize, isize) -> isize) -> bool {
    if (hwnd as usize) >= MAX_WINDOW_PROCS { return false; }
    unsafe {
        *WINDOW_PROCS[hwnd as usize].get() = Some(proc);
    }
    true
}

fn unregister_wndproc(hwnd: u32) {
    if (hwnd as usize) < MAX_WINDOW_PROCS {
        unsafe {
            *WINDOW_PROCS[hwnd as usize].get() = None;
        }
    }
}

fn call_window_proc(hwnd: u32, msg: u32, w_param: usize, l_param: isize) -> isize {
    if (hwnd as usize) < MAX_WINDOW_PROCS {
        unsafe {
            if let Some(proc) = *WINDOW_PROCS[hwnd as usize].get() {
                return proc(hwnd, msg, w_param, l_param);
            }
        }
    }
    match msg {
        WM_NCCREATE => 1,
        WM_DESTROY => 0,
        WM_CLOSE => 0,
        WM_PAINT => 0,
        _ => 0,
    }
}

#[repr(C)]
struct FramebufferInfo {
    address: u64,
    physical_address: u64,
    width: u32,
    height: u32,
    pitch: u32,
    red_shift: u8,
    green_shift: u8,
    blue_shift: u8,
    reserved: u8,
}

#[repr(C)]
struct InputEvent {
    kind: u32,
    key: u32,
    x: i32,
    y: i32,
    button: u32,
    pressed: u32,
}

#[repr(C)]
struct SurfaceEvent {
    surface: u32,
    kind: u32,
    x: i32,
    y: i32,
    button: u32,
    pressed: u32,
    key: u32,
}

fn syscall0(n: u64) -> u64 {
    let mut ret: u64;
    unsafe { core::arch::asm!("int 0x80", in("rax") n, lateout("rax") ret, options(nostack, nomem)); ret }
}

fn syscall1(n: u64, a1: u64) -> u64 {
    let mut ret: u64;
    unsafe { core::arch::asm!("int 0x80", in("rax") n, in("rdi") a1, lateout("rax") ret, options(nostack, nomem)); ret }
}

fn syscall2(n: u64, a1: u64, a2: u64) -> u64 {
    let mut ret: u64;
    unsafe { core::arch::asm!("int 0x80", in("rax") n, in("rdi") a1, in("rsi") a2, lateout("rax") ret, options(nostack, nomem)); ret }
}

fn console_write(bytes: &[u8]) {
    unsafe {
        core::arch::asm!("int 0x80", in("rax") SYS_WRITE, in("rdi") bytes.as_ptr(), in("rsi") bytes.len(), options(nostack, nomem));
    }
}

fn u32_to_bytes(n: u32, buf: &mut [u8]) -> usize {
    if n == 0 { buf[0] = b'0'; return 1; }
    let mut len = 0; let mut v = n;
    while v > 0 { buf[len] = b'0' + (v % 10) as u8; v /= 10; len += 1; }
    for i in 0..len / 2 { buf.swap(i, len - 1 - i); }
    len
}

fn console_write_u32(n: u32) {
    let mut buf = [0u8; 16];
    let len = u32_to_bytes(n, &mut buf);
    console_write(&buf[..len]);
}

#[unsafe(no_mangle)]
extern "C" fn _start(_argc: usize, _argv: *const *const u8) -> ! {
    console_write(b"ws_server: starting...\r\n");

    let mut fb_info = FramebufferInfo { address: 0, physical_address: 0, width: 0, height: 0, pitch: 0, red_shift: 0, green_shift: 0, blue_shift: 0, reserved: 0 };
    let ret = syscall1(SYS_FB_INFO, &mut fb_info as *mut FramebufferInfo as u64);
    console_write(b"ws_server: fb_info ret=");
    console_write_u32(ret as u32);
    console_write(b"\r\n");
    if ret != 0 {
        console_write(b"ws_server: falha ao obter info do framebuffer\n");
        unsafe { core::arch::asm!("int 0x80", in("rax") SYS_EXIT, options(nostack, nomem)); }
    }

    let width = fb_info.width;
    let height = fb_info.height;
    let pitch = fb_info.pitch;

    console_write(b"ws_server: framebuffer ");
    console_write_u32(width);
    console_write(b"x");
    console_write_u32(height);
    console_write(b", pitch ");
    console_write_u32(pitch);
    console_write(b"\n");

    let mut fb_map_info = FramebufferInfo { address: 0, physical_address: fb_info.physical_address, width, height, pitch, red_shift: fb_info.red_shift, green_shift: fb_info.green_shift, blue_shift: fb_info.blue_shift, reserved: 0 };
    let ret = syscall1(SYS_FB_MAP, &mut fb_map_info as *mut FramebufferInfo as u64);
    if ret != 0 {
        console_write(b"ws_server: falha ao mapear framebuffer\n");
        unsafe { core::arch::asm!("int 0x80", in("rax") SYS_EXIT, options(nostack, nomem)); }
    }

    let fb_addr = fb_map_info.address as *mut u32;
    let pitch_pixels = (pitch as usize) / 4;

    unsafe {
        core::arch::asm!("int 0x80", in("rax") SYS_CONSOLE_RELEASE, options(nostack, nomem));
    }

    console_write(b"ws_server: console liberado, servidor de display ativo\n");

    let mut hdc = HDC { surface: fb_addr, width, height, pitch_pixels };
    let mut wm = WindowManager::new();

    wm.add_window(Rect::new(0, 0, width as i32, height as i32), "Área de Trabalho");
    wm.add_window(Rect::new(150, 150, 450, 350), "Bloco de Notas");

    console_write(b"ws_server: janelas iniciais criadas\n");

    loop {
        let mut ev = InputEvent { kind: 0, key: 0, x: 0, y: 0, button: 0, pressed: 0 };
        let ev_ptr = &mut ev as *mut InputEvent as u64;

        loop {
            let has = syscall1(SYS_INPUT_POLL, ev_ptr);
            if has == 0 { break; }
            match ev.kind {
                INPUT_KEY => {
                    console_write(b"ws_server: tecla ");
                    console_write_u32(ev.key);
                    console_write(b"\n");
                    if let Some(active) = wm.active_window() {
                        call_window_proc(active, WM_KEYDOWN, ev.key as usize, 0);
                    }
                }
                INPUT_MOUSE_MOVE => { wm.set_cursor_pos(ev.x, ev.y); }
                INPUT_MOUSE_BUTTON => {
                    if ev.pressed != 0 { console_write(b"pressionado\n"); }
                    else { console_write(b"liberado\n"); }
                    let mut clicked: Option<u32> = None;
                    for slot in wm.windows_iter() {
                        let r = slot.rect;
                        if ev.x >= r.left && ev.x < r.right && ev.y >= r.top && ev.y < r.bottom {
                            clicked = Some(slot.id);
                            break;
                        }
                    }
                    if let Some(id) = clicked {
                        wm.set_active(id);
                        if ev.pressed != 0 {
                            call_window_proc(id, WM_LBUTTONDOWN, ev.button as usize, 0);
                        }
                    }
                }
                _ => {}
            }
        }

        let pos = syscall0(SYS_POINTER_POSITION);
        let cursor_x = (pos >> 32) as u32 as i32;
        let cursor_y = pos as u32 as i32;
        wm.set_cursor_pos(cursor_x, cursor_y);

        loop {
            let mut sev = SurfaceEvent { surface: 0, kind: 0, x: 0, y: 0, button: 0, pressed: 0, key: 0 };
            let sev_ptr = &mut sev as *mut SurfaceEvent as u64;
            let has = syscall1(SYS_SURFACE_POLL, sev_ptr);
            if has == 0 { break; }
            call_window_proc(sev.surface, sev.kind, sev.x as usize, sev.y as isize);
        }

        let bg_rect = Rect::new(0, 0, width as i32, height as i32);
        hdc.fill_rect(&bg_rect, COLOR_3DFACE);
        wm.render(&mut hdc);
        let _ = syscall2(SYS_SURFACE_PRESENT, 0, 1);

        for _ in 0..50_000 {
            core::hint::spin_loop();
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    console_write(b"ws_server: PANIC\n");
    loop { unsafe { core::arch::asm!("hlt", options(nomem, nostack)); } }
}
