use crate::asm;
use crate::fonts::*;
use core::fmt;

const COLOR_PALETTE: [[u8; 3]; 16] = [
	[0x00, 0x00, 0x00],	/*  0:黒 */
	[0xff, 0x00, 0x00],	/*  1:明るい赤 */
	[0x00, 0xff, 0x00],	/*  2:明るい緑 */
	[0xff, 0xff, 0x00],	/*  3:明るい黄色 */
	[0x00, 0x00, 0xff],	/*  4:明るい青 */
	[0xff, 0x00, 0xff],	/*  5:明るい紫 */
	[0x00, 0xff, 0xff],	/*  6:明るい水色 */
	[0xff, 0xff, 0xff],	/*  7:白 */
	[0xc6, 0xc6, 0xc6],	/*  8:明るい灰色 */
	[0x84, 0x00, 0x00],	/*  9:暗い赤 */
	[0x00, 0x84, 0x00],	/* 10:暗い緑 */
	[0x84, 0x84, 0x00],	/* 11:暗い黄色 */
	[0x00, 0x00, 0x84],	/* 12:暗い青 */
	[0x84, 0x00, 0x84],	/* 13:暗い紫 */
	[0x00, 0x84, 0x84],	/* 14:暗い水色 */
	[0x84, 0x84, 0x84]	/* 15:暗い灰色 */
];


#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Color {
    Black = 0,
    LightRed = 1,
    LightGreen = 2,
    LightYellow = 3,
    LightBlue = 4,
    LightPurple = 5,
    LightCyan = 6,
    White = 7,
    LightGray = 8,
    DarkRed = 9,
    DarkGreen = 10,
    DarkYellow = 11,
    DarkBlue = 12,
    DarkPurple = 13,
    DarkCyan = 14,
    DarkGray = 15,
}

#[derive(Debug)]
pub struct Screen {
    pub scrnx: i16,
    pub scrny: i16,
    pub vram: usize,
}


impl Screen {
    pub fn new() -> Screen {
        Screen {
            scrnx: unsafe { *(0x0ff4 as *const i16) },  // 画面横幅
            scrny: unsafe { *(0x0ff6 as *const i16) },  // 縦幅
            vram: unsafe { *(0x0ff8 as *const usize) },
        }
    }

    pub fn init(&mut self, buf: usize) {
        self.set_palette();
        self.init_screen(buf);
    }

    pub fn putblock(
        &mut self,
        buf: usize,
        bxsize: isize,  // バッファの幅
        image: [[Color; 16]; 16],
        pxsize: isize,
        pysize: isize,
        px0: isize,
        py0: isize,
    ) {
        for y in 0..pysize {
            for x in 0..pxsize {
                let ptr = unsafe { &mut *((buf as isize + (py0 + y) * bxsize + (px0 + x)) as *mut u8) };
                *ptr = image[y as usize][x as usize] as u8;
            }
        }
    }

    pub fn set_palette(&self) {
        let eflags = asm::load_eflags();
        asm::cli();
        asm::out8(0x03c8, 0);
        for i in 0..16 {
            // 書き込むときは上位2ビットを0にしないといけない
            asm::out8(0x03c9, COLOR_PALETTE[i][0] / 4);
            asm::out8(0x03c9, COLOR_PALETTE[i][1] / 4);
            asm::out8(0x03c9, COLOR_PALETTE[i][2] / 4);
        }
        asm::store_eflags(eflags);
    }

    pub fn init_screen(&mut self, buf: usize) {
        use Color::*;
        let xsize = self.scrnx as isize;
        let ysize = self.scrny as isize;

        self.boxfill8(buf, DarkCyan, 0, 0, xsize - 1, ysize - 29);
        self.boxfill8(buf, LightGray, 0, ysize - 28, xsize - 1, ysize - 28);
        self.boxfill8(buf, White, 0, ysize - 27, xsize - 1, ysize - 27);
        self.boxfill8(buf, LightGray, 0, ysize - 26, xsize - 1, ysize - 1);
    
        self.boxfill8(buf, White, 3, ysize - 24, 59, ysize - 24);
        self.boxfill8(buf, White, 2, ysize - 24, 2, ysize - 4);
        self.boxfill8(buf, DarkYellow, 3, ysize - 4, 59, ysize - 4);
        self.boxfill8(buf, DarkYellow, 59, ysize - 23, 59, ysize - 5);
        self.boxfill8(buf, Black,  2, ysize -  3, 59, ysize - 3);
        self.boxfill8(buf, Black, 60, ysize - 24, 60, ysize - 3);
    
        self.boxfill8(buf, DarkGray, xsize - 47, ysize - 24, xsize - 4, ysize - 24);
        self.boxfill8(buf, DarkGray, xsize - 47, ysize - 23, xsize - 47, ysize - 4);
        self.boxfill8(buf, White, xsize - 47, ysize - 3, xsize - 4, ysize - 3);
        self.boxfill8(buf, White, xsize - 3, ysize - 24, xsize - 3, ysize - 3);
    }

    pub fn boxfill8(&mut self, buf: usize, c: Color, x0: isize, y0: isize, x1: isize, y1: isize) { // x0..x1, y0..y1の範囲で四角を出力する
        for y in y0..y1+1 {
            for x in x0..x1+1 {
                let ptr = unsafe { &mut *((buf as isize + y * self.scrnx as isize + x) as *mut u8) };
                *ptr = c as u8;
            }
        }
    }

    pub fn print_char(&mut self, buf: usize, color: Color, x: isize, y: isize, font: u8) {
        let font = FONTS[font as usize];
        let color = color as u8;
        let xsize = self.scrnx as isize;
        let offset = x + y * xsize;
        for y in 0..FONT_HEIGHT {
            for x in 0..FONT_WIDTH {
                if font[y][x] == '*' {
                    let cell = (y * xsize as usize + x) as isize;
                    let ptr = unsafe { &mut *((buf as isize + cell + offset) as *mut u8) };
                    *ptr = color;
                }
            }
        }
    }
}



pub struct ScreenWriter {
    buf_addr: Option<usize>,
    initial_x: usize,
    x: usize,
    y: usize,
    color: Color,
    screen: Screen,
}

impl ScreenWriter {
    pub fn new(buf_addr: Option<usize>, screen: Screen, color: Color, x: usize, y: usize) -> ScreenWriter {
        ScreenWriter {
            buf_addr: buf_addr,
            initial_x: x,
            x,
            y,
            color,
            screen,
        }
    }

    fn newline(&mut self) {
        self.x = self.initial_x;
        self.y = self.y + FONT_HEIGHT;
    }
}

impl fmt::Write for ScreenWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let str_bytes = s.as_bytes();
        let height = self.screen.scrny as usize;
        let width = self.screen.scrnx as usize;
        for i in 0..str_bytes.len() {
            if str_bytes[i] == b'\n' {
                self.newline();
                return Ok(());
            }
            let buf_addr = if let Some(b) = self.buf_addr {
                b
            } else {
                self.screen.vram
            };
            if self.x + FONT_WIDTH < width && self.y + FONT_HEIGHT < height {
                self.screen
                    .print_char(buf_addr, self.color, self.x as isize, self.y as isize, str_bytes[i]);
            } else if self.y + FONT_HEIGHT * 2 < height {
                // 1行ずらせば入る場合は1行ずらしてから表示
                self.newline();
                self.screen
                    .print_char(buf_addr, self.color, self.x as isize, self.y as isize, str_bytes[i]);
            }
            // 次の文字用の位置に移動
            if self.x + FONT_WIDTH < width {
                self.x = self.x + FONT_WIDTH;
            } else if self.y + FONT_HEIGHT < height {
                self.newline();
            } else {
                self.x = width;
                self.y = height;
            }
        }
        Ok(())
    }
}