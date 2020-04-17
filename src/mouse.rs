use crate::vga::{Color, Screen};
use core::cell::{Cell, RefCell};

#[derive(Debug)]
pub struct MOUSE_DEC {
    pub buf: RefCell<[u8; 3]>,
    pub phase: Cell<u8>,
    pub x: Cell<i32>,
    pub y: Cell<i32>,
    pub btn: Cell<i32>,
}

impl MOUSE_DEC {
    pub fn new() -> MOUSE_DEC {
        MOUSE_DEC {
            buf: RefCell::new([0; 3]),
            phase: Cell::new(0),
            x: Cell::new(0),
            y: Cell::new(0),
            btn: Cell::new(0),
        }
    }

    pub fn mouse_decode(&self, dat: u8) -> Option<()> {
        if self.phase.get() == 0 {
            // マウスの0xfaを待っている段階
            if dat == 0xfa {
                self.phase.set(1);
            }
            return None
        } else if self.phase.get() == 1 {
            // マウスの1バイト目を待っている段階
            if dat & 0xc8 == 0x08 {
                let mut buf = self.buf.borrow_mut();
                buf[0] = dat;
                self.phase.set(2);
            }
            return None
        } else if self.phase.get() == 2 {
            // マウスの2バイト目を待っている段階
            let mut buf = self.buf.borrow_mut();
            buf[1] = dat;
            self.phase.set(3);
            return None
        } else if self.phase.get() == 3 {
            // マウスの3バイト目を待っている段階
            let mut buf = self.buf.borrow_mut();
            buf[2] = dat;
            self.phase.set(1);
            self.btn.set((buf[0] & 0x07) as i32);
            self.x.set(buf[1] as i32);
            self.y.set(buf[2] as i32);
            if buf[0] & 0x10 != 0 {
                self.x.set((buf[1] as u32 | 0xffffff00) as i32);
            }
            if buf[0] & 0x20 != 0 {
                self.y.set((buf[2] as u32 | 0xffffff00) as i32);
            }
            self.y.set(-self.y.get());   // マウスではy方向の符号が画面と反対
            return Some(())
        }
        None
    }
}

pub const MOUSE_CURSOR_WIDTH: usize = 16;
pub const MOUSE_CURSOR_HEIGHT: usize = 16;

#[derive(Debug)]
pub struct Mouse {
    x: Cell<i32>,
    y: Cell<i32>,
    cursor: [[Color; MOUSE_CURSOR_WIDTH]; MOUSE_CURSOR_HEIGHT],
}

impl Mouse {
    pub fn new(x: i32, y: i32) -> Mouse {
        let cursor_icon: [[u8; MOUSE_CURSOR_WIDTH]; MOUSE_CURSOR_HEIGHT] = [
            *b"**************..",
            *b"*OOOOOOOOOOO*...",
            *b"*OOOOOOOOOO*....",
            *b"*OOOOOOOOO*.....",
            *b"*OOOOOOOO*......",
            *b"*OOOOOOO*.......",
            *b"*OOOOOOO*.......",
            *b"*OOOOOOOO*......",
            *b"*OOOO**OOO*.....",
            *b"*OOO*..*OOO*....",
            *b"*OO*....*OOO*...",
            *b"*O*......*OOO*..",
            *b"**........*OOO*.",
            *b"*..........*OOO*",
            *b"............*OO*",
            *b".............***",
        ];

        let mut cursor: [[Color; MOUSE_CURSOR_WIDTH]; MOUSE_CURSOR_HEIGHT] =
            [[Color::DarkCyan; MOUSE_CURSOR_WIDTH]; MOUSE_CURSOR_HEIGHT];
        for y in 0..MOUSE_CURSOR_HEIGHT {
            for x in 0..MOUSE_CURSOR_WIDTH {
                match cursor_icon[y][x] {
                    b'*' => cursor[y][x] = Color::Black,
                    b'O' => cursor[y][x] = Color::White,
                    _ => (),
                }
            }
        }

        Mouse {
            x: Cell::new(x),
            y: Cell::new(y),
            cursor,
        }
    }

    pub fn move_and_render(&self, x: i32, y: i32) {
        // まず消す
        let mut screen = Screen::new();
        screen.boxfill8(
            Color::DarkCyan,
            self.x.get() as isize,
            self.y.get() as isize,
            (self.x.get() + MOUSE_CURSOR_WIDTH as i32 - 1) as isize,
            (self.y.get() + MOUSE_CURSOR_HEIGHT as i32 - 1) as isize,
        );
        // 移動
        let mx = self.x.get() + x;
        let my = self.y.get() + y;
        let xmax = screen.scrnx as i32 - MOUSE_CURSOR_WIDTH as i32;
        let ymax = screen.scrny as i32 - MOUSE_CURSOR_HEIGHT as i32;
        if mx < 0 {
            self.x.set(0);
        } else if mx > xmax {
            self.x.set(xmax);
        } else {
            self.x.set(mx);
        }
        if my < 0 {
            self.y.set(0);
        } else if my > ymax {
            self.y.set(ymax);
        } else {
            self.y.set(my);
        }
        // 現在位置で描画
        self.render();
    }

    pub fn render(&self) {
        Screen::new().putblock(
            self.cursor,
            MOUSE_CURSOR_WIDTH as isize,
            MOUSE_CURSOR_HEIGHT as isize,
            self.x.get() as isize,
            self.y.get() as isize,
        );
    }
}