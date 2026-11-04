//! Gerenciador de janelas simples para o servidor de display em userspace.
//! Mantém a lista de janelas (superfícies), z-order, foco e renderiza
//! usando o GDI local.

use crate::gdi::{HDC, Rect, COLOR_3DFACE, COLOR_BLACK, COLOR_BLUE, COLOR_3DSHADOW, COLOR_WHITE, draw_cursor};

const MAX_WINDOWS: usize = 16;

#[derive(Clone, Copy)]
pub struct Window {
    pub id: u32,
    pub rect: Rect,
    pub title: [u8; 64],
    pub title_len: usize,
    pub visible: bool,
    pub active: bool,
    pub z: u32,
}

impl Window {
    pub const fn new(id: u32) -> Self {
        Window {
            id, rect: Rect::new(100,100,300,200), title: [0;64], title_len: 0,
            visible: true, active: false, z: 0,
        }
    }

    fn title_str(&self) -> &str {
        core::str::from_utf8(&self.title[..self.title_len]).unwrap_or("Window")
    }
}

pub struct WindowManager {
    windows: [Option<Window>; MAX_WINDOWS],
    count: usize,
    next_id: u32,
    active_id: Option<u32>,
    cursor_x: i32,
    cursor_y: i32,
}

impl WindowManager {
    pub const fn new() -> Self {
        WindowManager {
            windows: [None; MAX_WINDOWS], count: 0, next_id: 1,
            active_id: None, cursor_x: 100, cursor_y: 100,
        }
    }

    pub fn add_window(&mut self, rect: Rect, title: &str) -> Option<u32> {
        if self.count >= MAX_WINDOWS { return None; }
        let id = self.next_id; self.next_id += 1;
        let idx = self.windows.iter().position(|w| w.is_none())?;
        let mut w = Window::new(id);
        w.rect = rect;
        let bytes = title.as_bytes();
        let len = bytes.len().min(63);
        w.title[..len].copy_from_slice(&bytes[..len]);
        w.title_len = len;
        w.active = self.active_id.is_none();
        if w.active { self.active_id = Some(id); }
        w.z = id;
        self.windows[idx] = Some(w);
        self.count += 1;
        Some(id)
    }

    pub fn set_active(&mut self, id: u32) {
        for w in self.windows.iter_mut().flatten() {
            w.active = (w.id == id);
        }
        self.active_id = Some(id);
    }

    pub fn set_cursor_pos(&mut self, x: i32, y: i32) {
        self.cursor_x = x; self.cursor_y = y;
    }

    pub fn active_window(&self) -> Option<u32> {
        self.active_id
    }

    pub fn windows_iter(&self) -> impl Iterator<Item = &Window> {
        self.windows.iter().flatten()
    }

    pub fn render(&self, hdc: &mut HDC) {
        let mut visible_ids: [u32; MAX_WINDOWS] = [0; MAX_WINDOWS];
        let mut visible_count = 0;
        for slot in self.windows.iter().flatten() {
            if slot.visible { visible_ids[visible_count] = slot.z; visible_count += 1; }
        }
        for i in 1..visible_count {
            let mut j = i;
            while j > 0 {
                let z_a = self.z_of(visible_ids[j-1]);
                let z_b = self.z_of(visible_ids[j]);
                if z_a > z_b { visible_ids.swap(j-1,j); j -= 1; } else { break; }
            }
        }
        for i in 0..visible_count {
            let id = visible_ids[i];
            if let Some(w) = self.window_by_id(id) {
                Self::draw_window(hdc, w);
            }
        }
        draw_cursor(hdc, self.cursor_x, self.cursor_y);
    }

    fn z_of(&self, id: u32) -> u32 {
        self.windows.iter().flatten().find(|w| w.id == id).map(|w| w.z).unwrap_or(0)
    }

    fn window_by_id(&self, id: u32) -> Option<&Window> {
        self.windows.iter().flatten().find(|w| w.id == id)
    }

    fn draw_window(hdc: &mut HDC, w: &Window) {
        let r = &w.rect;
        let border_color = if w.active { COLOR_BLACK } else { COLOR_3DSHADOW };
        let title_color = if w.active { COLOR_BLUE } else { COLOR_3DSHADOW };
        hdc.fill_rect(r, border_color);
        let client = Rect::new(r.left+2, r.top+24, r.right-2, r.bottom-2);
        hdc.fill_rect(&client, COLOR_3DFACE);
        let title_bar = Rect::new(r.left+2, r.top+2, r.right-2, r.top+22);
        hdc.fill_rect(&title_bar, title_color);
        hdc.text_out(r.left+6, r.top+4, w.title_str(), COLOR_WHITE);
        let xb = r.right - 16;
        let yb = r.top + 4;
        for i in 0..8 {
            hdc.put_pixel(xb+i, yb+i, COLOR_BLACK);
            hdc.put_pixel(xb+7-i, yb+i, COLOR_BLACK);
        }
    }
}
