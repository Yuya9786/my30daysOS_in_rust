#![no_std]
#![feature(asm)]
#![feature(start)]
#![feature(naked_functions)]

use core::panic::PanicInfo;
use lazy_static::lazy_static;

mod vga;
mod asm;
mod fonts;
mod dsctbl;
mod int;
mod fifo;
mod mouse;
mod mem;
mod sheet;
mod timer;


pub struct BOOTINFO {
    cyls: u8,
    leds: u8,
    vmode: u8,
    reserve: u8,
    scrnx: i16,
    scrny: i16,
    vram: usize,
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
            vram: unsafe { *(0x0ff8 as *const usize) }
        }
    }
}

lazy_static! {
    pub static ref binfo: BOOTINFO = BOOTINFO::new();
}

#[no_mangle]
#[start]
pub extern "C" fn HariMain() {
    use vga::{Color, ScreenWriter};
    use asm::{hlt, sti, cli, stihlt};
    use int::{keyfifo, mousefifo};
    use core::fmt::Write;
    use mouse::{Mouse, MOUSE_DEC, MOUSE_CURSOR_HEIGHT, MOUSE_CURSOR_WIDTH};
    use mem::{MEMMAN, MEMMAN_ADDR};
    use sheet::SHTCTL;
    use timer::timerctl;
    use fifo::FIFO8;

    let timerfifo1 = FIFO8::new(8);
    let timerfifo2 = FIFO8::new(8);
    let timerfifo3 = FIFO8::new(8);


    dsctbl::init();
    int::init_pic();
    sti();
    timer::init_pit();
    int::allow_input();
    vga::set_palette();
    int::enable_mouse();
    
    let timer_index1 = timerctl.lock().alloc().unwrap();
    timerctl.lock().init(timer_index1, &timerfifo1, 1);
    timerctl.lock().settime(timer_index1, 1000);
    let timer_index2 = timerctl.lock().alloc().unwrap();
    timerctl.lock().init(timer_index2, &timerfifo2, 1);
    timerctl.lock().settime(timer_index2, 300);
    let timer_index3 = timerctl.lock().alloc().unwrap();
    timerctl.lock().init(timer_index3, &timerfifo3, 1);
    timerctl.lock().settime(timer_index3, 50);
    
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
    let sht_map_addr = memman.alloc_4k(binfo.scrnx as u32 * binfo.scrny as u32);
    *shtctl = SHTCTL::new(sht_map_addr as usize);
    let sht_back = shtctl.alloc().unwrap();     // 背景
    let sht_mouse = shtctl.alloc().unwrap();    // マウス
    let sht_win = shtctl.alloc().unwrap();      // ウィンドウ
    let buf_addr_back = memman.alloc_4k(binfo.scrnx as u32 * binfo.scrny as u32) as usize;
    let buf_mouse = [1u8; MOUSE_CURSOR_HEIGHT * MOUSE_CURSOR_WIDTH];
    let buf_addr_mouse = &buf_mouse as *const [u8; MOUSE_CURSOR_HEIGHT * MOUSE_CURSOR_WIDTH] as usize;
    let buf_addr_win = memman.alloc_4k((160 * 52) as u32) as usize;
    shtctl.sheets_data[sht_back].setbuf(buf_addr_back, binfo.scrnx as usize, binfo.scrny as usize, None); // 透明色なし
    shtctl.sheets_data[sht_mouse].setbuf(buf_addr_mouse, 16, 16, Some(Color::DarkCyan));
    shtctl.sheets_data[sht_win].setbuf(buf_addr_win, 160, 52, None);
    
    vga::init_screen(buf_addr_back);

    let mdec =  MOUSE_DEC::new();   // マウスの0xfaを待っている段階へ
    let mouse = Mouse::new(
        buf_addr_mouse,
    );
    let mut mx = (binfo.scrnx as i32 - MOUSE_CURSOR_WIDTH as i32) / 2;    // 画面中央になるように座標計算
    let mut my = (binfo.scrny as i32 - MOUSE_CURSOR_HEIGHT as i32 - 28) / 2;
    mouse.render();

    vga::make_window(buf_addr_win as usize, 160, 52, "counter");

    shtctl.slide(sht_mouse, mx, my);
    shtctl.slide(sht_win, 80, 72);
    shtctl.updown(sht_back, Some(0));
    shtctl.updown(sht_win, Some(1));
    shtctl.updown(sht_mouse, Some(2));
    
    let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 0, 32, binfo.scrnx as usize, binfo.scrny as usize);
    write!(writer, "memory: {}MB free: {}KB ", memtotal / (1024 * 1024), memman.total() / 1024).unwrap();
    shtctl.refresh(sht_back, 0, 0, binfo.scrnx as i32, 48);
    loop {
        cli();
        if let Some(tc) = timerctl.try_lock() {
            vga::boxfill8(buf_addr_win, 160, Color::LightGray, 40, 28, 119, 43);
            let mut writer = ScreenWriter::new(Some(buf_addr_win), Color::Black, 40, 28, 160, 52);
            write!(writer, "{:>010}", tc.count).unwrap();
            shtctl.refresh(sht_win, 40, 28, 120, 44);
        }
        
        if keyfifo.lock().status() != 0 {
            let i = keyfifo.lock().get().unwrap();
            sti();
            vga::boxfill8(buf_addr_back as usize, binfo.scrnx as isize, Color::DarkCyan, 0, 0, 16, 16);
            let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 0, 0, binfo.scrnx as usize, binfo.scrny as usize);
            write!(writer, "{:02x}", i).unwrap();
            shtctl.refresh(sht_back, 0, 0, 16, 16);
        } else if mousefifo.lock().status() != 0 {
            let i = mousefifo.lock().get().unwrap();
            sti();
            if mdec.mouse_decode(i).is_some() {
                // データが3バイト揃ったので表示
                vga::boxfill8(buf_addr_back as usize, binfo.scrnx as isize, Color::DarkCyan, 32, 0, 32 + 15 * 8, 16);
                let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 32, 0, binfo.scrnx as usize, binfo.scrny as usize);
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
                if mx > binfo.scrnx as i32 - 1 {
                    mx = binfo.scrnx as i32 - 1;
                }
                if my > binfo.scrny as i32 - 1 {
                    my = binfo.scrny as i32 - 1;
                }
                shtctl.refresh(sht_back, 32, 0, 32 + 15 * 8, 16);
                shtctl.slide(sht_mouse, mx, my);
            }
        } else if timerfifo1.status() != 0 {
            let _ = timerfifo1.get();
            sti();
            let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 0, 64, binfo.scrnx as usize, binfo.scrny as usize);
            write!(writer, "10[sec]").unwrap();
            shtctl.refresh(sht_back, 0, 64, 56, 80);
        } else if timerfifo2.status() != 0 {
            let _ = timerfifo2.get();
            sti();
            let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 0, 80, binfo.scrnx as usize, binfo.scrny as usize);
            write!(writer, "3[sec]").unwrap();
            shtctl.refresh(sht_back, 0, 80, 48, 96);
        } else if timerfifo3.status() != 0 {
            let i = timerfifo3.get().unwrap();
            sti();
            if i != 0 {
                timerctl.lock().init(timer_index3, &timerfifo3, 0);
                vga::boxfill8(buf_addr_back as usize, binfo.scrnx as isize, Color::White, 8, 96, 15, 111);
            } else {
                timerctl.lock().init(timer_index3, &timerfifo3, 1);
                vga::boxfill8(buf_addr_back as usize, binfo.scrnx as isize, Color::DarkCyan, 8, 96, 15, 111);
            }
            timerctl.lock().settime(timer_index3, 50);
            shtctl.refresh(sht_back, 8, 96, 16, 112);
        } else {
            sti();
        }
    }
}


#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use vga::ScreenWriter;
    let mut writer = ScreenWriter::new(None, vga::Color::LightRed, 0, 0, binfo.scrnx as usize, binfo.scrny as usize);
    use core::fmt::Write;
    write!(writer, "[ERR] {:?}", info).unwrap();
    loop {
        asm::hlt()
    }
}