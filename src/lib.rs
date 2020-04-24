#![no_std]
#![feature(asm)]
#![feature(start)]
#![feature(naked_functions)]

use core::panic::PanicInfo;

mod vga;
mod asm;
mod fonts;
mod dsctbl;
mod int;
mod fifo;
mod mouse;
mod mem;
mod sheet;

#[no_mangle]
fn write_mem8(addr: u32, data: u8) {
    let ptr = unsafe { &mut *(addr as *mut u8) };
    *ptr = data;
}

struct BOOTINFO {
    cyls: u8,
    leds: u8,
    vmode: u8,
    reserve: u8,
    scrnx: i16,
    scrny: i16,
    vram: &'static mut u8,
}

impl BOOTINFO {
    pub fn new() -> BOOTINFO {
        BOOTINFO {
            cyls: unsafe { *(0x0ff0 as *const u8) },
            leds: unsafe { *(0x0ff1 as *const u8) },
            vmode: unsafe { *(0x0ff2 as *const u8) },
            reserve: unsafe { *(0x0ff3 as *const u8) },
            scrnx: unsafe { *(0x0ff4 as *const i16) },
            scrny: unsafe { *(0x0ff6 as *const i16) },
            vram: unsafe { &mut *( *(0x0ff8 as *const i32) as *mut u8) }
        }
    }
}

#[no_mangle]
#[start]
pub extern "C" fn HariMain() {
    use vga::{Color, Screen, ScreenWriter};
    use asm::{hlt, sti, cli, stihlt};
    use int::{keyfifo, mousefifo};
    use core::fmt::Write;
    use mouse::{Mouse, MOUSE_DEC, MOUSE_CURSOR_HEIGHT, MOUSE_CURSOR_WIDTH};
    use mem::{MEMMAN, MEMMAN_ADDR};
    use sheet::SHTCTL;

    let binfo = BOOTINFO::new();

    dsctbl::init();
    int::init_pic();
    sti();
    int::allow_input();
    
    let memtotal = mem::memtest(0x00400000, 0xbfffffff);
    let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MEMMAN) };
    *memman = MEMMAN::new();
    memman.free(0x00001000, 0x0009e000);   // 0x00001000 - 0x0009efff
    memman.free(0x00400000, 2);
    memman.free(0x00400000, memtotal  - 0x00400000);

    let shtctl = unsafe {
        &mut *(memman
            .alloc_4k(core::mem::size_of::<SHTCTL>() as u32) as *mut SHTCTL)
    };
    *shtctl = SHTCTL::new(binfo.vram, binfo.scrnx as i32, binfo.scrny as i32);
    let sht_back = shtctl.alloc().unwrap();  // 背景
    let sht_mouse = shtctl.alloc().unwrap(); // マウス
    let buf_addr_back = memman.alloc_4k((binfo.scrnx as u32 * binfo.scrny as u32));
    let buf_mouse = [1u8; MOUSE_CURSOR_HEIGHT * MOUSE_CURSOR_WIDTH];
    let buf_addr_mouse = &buf_mouse as *const [u8; MOUSE_CURSOR_HEIGHT * MOUSE_CURSOR_WIDTH] as usize;
    shtctl.sheets_data[sht_back].setbuf(buf_addr_back as usize, binfo.scrnx as usize, binfo.scrny as usize, None); // 透明色なし
    shtctl.sheets_data[sht_mouse].setbuf(buf_addr_mouse, 16, 16, Some(Color::DarkCyan));
    
    let mut screen = Screen::new();
    screen.init(buf_addr_back as usize);
    int::enable_mouse();

    let mdec =  MOUSE_DEC::new();   // マウスの0xfaを待っている段階へ
    let mouse = Mouse::new(
        buf_addr_mouse,
    );
    mouse.render();

    let mut mx = (binfo.scrnx as i32 - MOUSE_CURSOR_WIDTH as i32) / 2;    // 画面中央になるように座標計算
    let mut my = (binfo.scrny as i32 - MOUSE_CURSOR_HEIGHT as i32 - 28) / 2;
    shtctl.slide(sht_mouse, mx, my);
    shtctl.updown(sht_back, Some(0));
    shtctl.updown(sht_mouse, Some(1));

    (Screen::new()).boxfill8(buf_addr_back as usize, Color::DarkCyan, 0, 32, 100, 48);
    let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Screen::new(), Color::White, 0, 32);
    write!(writer, "memory: {}MB free: {}KB ", memtotal / (1024 * 1024), memman.total() / 1024).unwrap();
    shtctl.refresh(sht_back, 0, 0, binfo.scrnx as i32, 48);
    loop {
        cli();
        if keyfifo.lock().status() + mousefifo.lock().status() == 0 {
            stihlt();
        } else {
            if keyfifo.lock().status() != 0 {
                let i = keyfifo.lock().get().unwrap();
                sti();
                (Screen::new()).boxfill8(buf_addr_back as usize, Color::DarkCyan, 0, 0, 16, 16);
                let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Screen::new(), Color::White, 0, 0);
                write!(writer, "{:02x}", i).unwrap();
                shtctl.refresh(sht_back, 0, 0, 16, 16);
            } else if mousefifo.lock().status() != 0 {
                let i = mousefifo.lock().get().unwrap();
                sti();
                if mdec.mouse_decode(i).is_some() {
                    // データが3バイト揃ったので表示
                    (Screen::new()).boxfill8(buf_addr_back as usize, Color::DarkCyan, 32, 0, 32 + 15 * 8, 16);
                    let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Screen::new(), Color::White, 32, 0);
                    write!(writer, "[{}{}{} {:>4} {:>4}]", 
                    if mdec.btn.get() & 0x01 != 0 {
                        'L'
                    } else {
                        'l'
                    },
                    if mdec.btn.get() & 0x04 != 0 {
                        'C'
                    } else {
                        'c'
                    },
                    if mdec.btn.get() & 0x02 != 0 {
                        'R'
                    } else {
                        'r'
                    },
                    mdec.x.get(), mdec.y.get()
                    ).unwrap();

                    mx += mdec.x.get();
                    my += mdec.y.get();
                    if mx < 0 {
                        mx = 0;
                    }
                    if my < 0 {
                        my = 0;
                    }
                    if mx > binfo.scrnx as i32 - 16 {
                        mx = binfo.scrnx as i32 - 16;
                    }
                    if my > binfo.scrny as i32 - 16 {
                        my = binfo.scrny as i32 - 16;
                    }
                    shtctl.refresh(sht_back, 32, 0, 32 + 15 * 8, 16);
                    shtctl.slide(sht_mouse, mx, my);
                }
            }
        }
    }
}


#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use vga::{Screen, ScreenWriter};
    let mut writer = ScreenWriter::new(None, Screen::new(), vga::Color::LightRed, 0, 0);
    use core::fmt::Write;
    write!(writer, "[ERR] {:?}", info).unwrap();
    loop {
        asm::hlt()
    }
}