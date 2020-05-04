#![no_std]
#![feature(asm)]
#![feature(start)]
#![feature(naked_functions)]

use core::panic::PanicInfo;
use lazy_static::lazy_static;

mod vga;
mod asm;
mod fonts;
mod descriptor_table;
mod interrupt;
mod fifo;
mod mouse;
mod memory;
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
    use asm::{cli, sti};
    use core::fmt::Write;
    use fifo::Fifo;
    use interrupt::{enable_mouse, KEYBUF, MOUSEBUF};
    use memory::{MemMan, MEMMAN_ADDR};
    use mouse::{Mouse, MouseDec, MOUSE_CURSOR_HEIGHT, MOUSE_CURSOR_WIDTH};
    use sheet::SheetManager;
    use timer::TIMER_MANAGER;
    use vga::{
        boxfill, init_palette, init_screen, make_window, Color, ScreenWriter, SCREEN_HEIGHT,
        SCREEN_WIDTH,
    };

    let timerfifo1 = Fifo::new(8);
    let timerfifo2 = Fifo::new(8);
    let timerfifo3 = Fifo::new(8);


    descriptor_table::init();
    interrupt::init();
    sti();
    timer::init_pit();
    interrupt::allow_input();
    vga::init_palette();
    enable_mouse();
    
    let timer_index1 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index1, &timerfifo1, 1);
    TIMER_MANAGER.lock().set_time(timer_index1, 1000);
    let timer_index2 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index2, &timerfifo2, 1);
    TIMER_MANAGER.lock().set_time(timer_index2, 300);
    let timer_index3 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index3, &timerfifo3, 1);
    TIMER_MANAGER.lock().set_time(timer_index3, 50);
    
    let memtotal = memory::memtest(0x00400000, 0xbfffffff);
    let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
    *memman = MemMan::new();
    memman.free(0x00001000, 0x0009e000);   // 0x00001000 - 0x0009efff
    memman.free(0x00400000, 2);
    memman.free(0x00400000, memtotal  - 0x00400000);

    let shtctl = unsafe {
        &mut *(memman
            .alloc_4k(core::mem::size_of::<SheetManager>() as u32).unwrap() as *mut SheetManager)
    };
    let sht_map_addr = memman.alloc_4k(binfo.scrnx as u32 * binfo.scrny as u32).unwrap();
    *shtctl = SheetManager::new(sht_map_addr as i32);
    let sht_back = shtctl.alloc().unwrap();     // 背景
    let sht_mouse = shtctl.alloc().unwrap();    // マウス
    let sht_win = shtctl.alloc().unwrap();      // ウィンドウ
    let buf_addr_back = memman.alloc_4k(binfo.scrnx as u32 * binfo.scrny as u32).unwrap() as usize;
    let buf_mouse = [1u8; MOUSE_CURSOR_HEIGHT * MOUSE_CURSOR_WIDTH];
    let buf_addr_mouse = &buf_mouse as *const [u8; MOUSE_CURSOR_HEIGHT * MOUSE_CURSOR_WIDTH] as usize;
    let buf_addr_win = memman.alloc_4k((160 * 52) as u32).unwrap() as usize;
    shtctl.sheets_data[sht_back].set(buf_addr_back, binfo.scrnx as i32, binfo.scrny as i32, None); // 透明色なし
    shtctl.sheets_data[sht_mouse].set(buf_addr_mouse, 16, 16, Some(Color::DarkCyan));
    shtctl.sheets_data[sht_win].set(buf_addr_win, 160, 52, None);
    
    vga::init_screen(buf_addr_back as usize);

    let mdec =  MouseDec::new();   // マウスの0xfaを待っている段階へ
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
        if let Some(tc) = TIMER_MANAGER.try_lock() {
            boxfill(buf_addr_win, 160, Color::LightGray, 40, 28, 119, 43);
            let mut writer = ScreenWriter::new(Some(buf_addr_win), Color::Black, 40, 28, 160, 52);
            write!(writer, "{:>010}", tc.count).unwrap();
            shtctl.refresh(sht_win, 40, 28, 120, 44);
        }
        
        if KEYBUF.lock().status() != 0 {
            let i = KEYBUF.lock().get().unwrap();
            sti();
            boxfill(buf_addr_back as usize, binfo.scrnx as isize, Color::DarkCyan, 0, 0, 16, 16);
            let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 0, 0, binfo.scrnx as usize, binfo.scrny as usize);
            write!(writer, "{:02x}", i).unwrap();
            shtctl.refresh(sht_back, 0, 0, 16, 16);
        } else if MOUSEBUF.lock().status() != 0 {
            let i = MOUSEBUF.lock().get().unwrap();
            sti();
            if mdec.decode(i).is_some() {
                // データが3バイト揃ったので表示
                boxfill(buf_addr_back as usize, binfo.scrnx as isize, Color::DarkCyan, 32, 0, 32 + 15 * 8, 16);
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
            let _ = timerfifo1.get().unwrap();
            sti();
            let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 0, 64, binfo.scrnx as usize, binfo.scrny as usize);
            write!(writer, "10[sec]").unwrap();
            shtctl.refresh(sht_back, 0, 64, 56, 80);
        } else if timerfifo2.status() != 0 {
            let _ = timerfifo2.get().unwrap();
            sti();
            let mut writer = ScreenWriter::new(Some(buf_addr_back as usize), Color::White, 0, 80, binfo.scrnx as usize, binfo.scrny as usize);
            write!(writer, "3[sec]").unwrap();
            shtctl.refresh(sht_back, 0, 80, 48, 96);
        } else if timerfifo3.status() != 0 {
            let i = timerfifo3.get().unwrap();
            sti();
            if i != 0 {
                TIMER_MANAGER.lock().init_timer(timer_index3, &timerfifo3, 0);
                boxfill(buf_addr_back, *SCREEN_WIDTH as isize, Color::White, 8, 96, 15, 111);
            } else {
                TIMER_MANAGER.lock().init_timer(timer_index3, &timerfifo3, 1);
                boxfill(buf_addr_back, *SCREEN_WIDTH as isize, Color::DarkCyan, 8, 96, 15, 111);
            }
            TIMER_MANAGER.lock().set_time(timer_index3, 50);
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