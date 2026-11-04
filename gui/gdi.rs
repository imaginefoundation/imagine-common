//! GDI simplificado para o servidor de janelas em userspace.
//! Desenha diretamente no buffer do framebuffer mapeado pelo kernel.

pub const COLOR_3DFACE: u32 = 0xC0C0C0;
pub const COLOR_3DHILIGHT: u32 = 0xFFFFFF;
pub const COLOR_3DSHADOW: u32 = 0x808080;
pub const COLOR_WINDOWTEXT: u32 = 0x000000;
pub const COLOR_BLACK: u32 = 0x000000;
pub const COLOR_WHITE: u32 = 0xFFFFFF;
pub const COLOR_BLUE: u32 = 0x0000FF;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub const fn new(l: i32, t: i32, r: i32, b: i32) -> Self {
        Rect { left: l, top: t, right: r, bottom: b }
    }
}

pub struct HDC {
    pub surface: *mut u32,
    pub width: u32,
    pub height: u32,
    pub pitch_pixels: usize,
}

impl HDC {
    pub fn null() -> Self {
        HDC { surface: core::ptr::null_mut(), width: 0, height: 0, pitch_pixels: 0 }
    }

    pub fn is_valid(&self) -> bool { !self.surface.is_null() }

    pub fn put_pixel(&mut self, x: i32, y: i32, color: u32) {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 { return; }
        if self.surface.is_null() { return; }
        unsafe {
            let offset = (y as usize * self.pitch_pixels) + (x as usize);
            *self.surface.offset(offset as isize) = color;
        }
    }

    pub fn fill_rect(&mut self, rect: &Rect, color: u32) {
        if !self.is_valid() { return; }
        let top = rect.top.max(0).min(self.height as i32);
        let bottom = rect.bottom.max(0).min(self.height as i32);
        let left = rect.left.max(0).min(self.width as i32);
        let right = rect.right.max(0).min(self.width as i32);
        for y in top..bottom { for x in left..right { self.put_pixel(x, y, color); } }
    }

    pub fn hline(&mut self, x1: i32, x2: i32, y: i32, color: u32) {
        let start = x1.min(x2).max(0);
        let end = x1.max(x2).min(self.width as i32);
        if y < 0 || y >= self.height as i32 { return; }
        for x in start..end { self.put_pixel(x, y, color); }
    }

    pub fn vline(&mut self, x: i32, y1: i32, y2: i32, color: u32) {
        let start = y1.min(y2).max(0);
        let end = y1.max(y2).min(self.height as i32);
        if x < 0 || x >= self.width as i32 { return; }
        for y in start..end { self.put_pixel(x, y, color); }
    }

    pub fn draw_rect(&mut self, rect: &Rect, color: u32) {
        self.hline(rect.left, rect.right, rect.top, color);
        self.hline(rect.left, rect.right, rect.bottom - 1, color);
        self.vline(rect.left, rect.top, rect.bottom, color);
        self.vline(rect.right - 1, rect.top, rect.bottom, color);
    }

    pub fn text_out(&mut self, x: i32, y: i32, text: &str, color: u32) {
        let mut cx = x;
        for ch in text.chars() { self.draw_char(cx, y, ch, color); cx += 8; }
    }

    fn draw_char(&mut self, x: i32, y: i32, ch: char, color: u32) {
        let pattern: [u8; 8] = match ch {
            'A' => [0x18,0x24,0x24,0x3C,0x24,0x24,0x24,0x00],
            'B' => [0x3C,0x24,0x24,0x38,0x24,0x24,0x3C,0x00],
            'C' => [0x3C,0x20,0x20,0x20,0x20,0x20,0x3C,0x00],
            'D' => [0x38,0x24,0x24,0x24,0x24,0x24,0x38,0x00],
            'E' => [0x3C,0x20,0x20,0x38,0x20,0x20,0x3C,0x00],
            'F' => [0x3C,0x20,0x20,0x38,0x20,0x20,0x20,0x00],
            'G' => [0x3C,0x20,0x20,0x24,0x24,0x24,0x3C,0x00],
            'H' => [0x24,0x24,0x24,0x3C,0x24,0x24,0x24,0x00],
            'I' => [0x3C,0x08,0x08,0x08,0x08,0x08,0x08,0x3C],
            'L' => [0x20,0x20,0x20,0x20,0x20,0x20,0x3C,0x00],
            'M' => [0x24,0x36,0x36,0x24,0x24,0x24,0x24,0x00],
            'N' => [0x24,0x34,0x34,0x2C,0x2C,0x26,0x26,0x00],
            'O' => [0x3C,0x24,0x24,0x24,0x24,0x24,0x3C,0x00],
            'P' => [0x3C,0x24,0x24,0x3C,0x20,0x20,0x20,0x00],
            'R' => [0x3C,0x24,0x24,0x3C,0x28,0x24,0x24,0x00],
            'S' => [0x3C,0x20,0x20,0x3C,0x04,0x04,0x3C,0x00],
            'T' => [0x3C,0x08,0x08,0x08,0x08,0x08,0x08,0x00],
            'W' => [0x24,0x24,0x24,0x24,0x2A,0x2A,0x2A,0x00],
            _ => [0x3C,0x24,0x24,0x3C,0x24,0x24,0x3C,0x00],
        };
        for dy in 0..8 {
            let row = pattern[dy];
            for dx in 0..8 {
                if (row & (0x80 >> dx)) != 0 {
                    self.put_pixel(x + dx as i32, y + dy as i32, color);
                }
            }
        }
    }
}

pub fn draw_cursor(hdc: &mut HDC, x: i32, y: i32) {
    let bitmap: [u32; 22] = [
        0x80000000,0xC0000000,0xE0000000,0xF0000000,
        0xF8000000,0xFC000000,0xFE000000,0xFF000000,
        0xFF800000,0xFFC00000,0xFFE00000,0xFFF00000,
        0xFFF80000,0xFFFC0000,0xFFFE0000,0xFFFF0000,
        0xFFFF8000,0xFFFFC000,0x00000000,0x00000000,
        0x00000000,0x00000000,
    ];
    for dy in 0..22 {
        let row = bitmap[dy];
        for dx in 0..32 {
            if (row & (0x80000000 >> dx)) != 0 {
                let cx = x + dx as i32;
                let cy = y + dy as i32;
                if cx >= 0 && cx < hdc.width as i32 && cy >= 0 && cy < hdc.height as i32 {
                    let is_border = dx == 0 || dy == 0 || dx == 31 || dy == 21;
                    if is_border { hdc.put_pixel(cx, cy, COLOR_BLACK); }
                    else { hdc.put_pixel(cx, cy, COLOR_WHITE); }
                }
            }
        }
    }
}
