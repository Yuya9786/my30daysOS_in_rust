#![no_std]
#![feature(asm)]
#![feature(start)]
#![feature(naked_functions)]
#![feature(alloc_error_handler)]

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
//mod allocator;


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
    use fifo::FIFO_BUF;
    use interrupt::enable_mouse;
    use memory::{MemMan, MEMMAN_ADDR};
    use mouse::{Mouse, MouseDec, MOUSE_CURSOR_HEIGHT, MOUSE_CURSOR_WIDTH};
    use sheet::SheetManager;
    use timer::TIMER_MANAGER;
    use vga::{
        boxfill, init_palette, init_screen, make_window, Color, ScreenWriter, SCREEN_HEIGHT,
        SCREEN_WIDTH,
    };

    descriptor_table::init();
    interrupt::init();
    sti();
    timer::init_pit();
    interrupt::allow_input();
    vga::init_palette();
    enable_mouse();
    
    let timer_index1 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index1, 10);
    TIMER_MANAGER.lock().set_time(timer_index1, 1000);
    let timer_index2 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index2, 3);
    TIMER_MANAGER.lock().set_time(timer_index2, 300);
    let timer_index3 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index3, 1);
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

    write_with_bg!(
        shtctl,
        sht_back,
        buf_addr_back,
        *SCREEN_WIDTH as isize,
        *SCREEN_HEIGHT as isize,
        0,
        32,
        Color::White,
        Color::DarkCyan,
        27,
        "total: {}MB  free: {}KB",
        memtotal / (1024 * 1024),
        memman.total() / 1024
    );

    let mut count = 0;
    let mut count_done = false;
    loop {
        count += 1;
        cli();
        
        if FIFO_BUF.lock().status() != 0 {
            let i = FIFO_BUF.lock().get().unwrap();
            sti();
            if 256 <= i && i <= 511 {
                write_with_bg!(
                    shtctl,
                    sht_back,
                    buf_addr_back,
                    *SCREEN_WIDTH as isize,
                    *SCREEN_HEIGHT as isize,
                    0,
                    0,
                    Color::White,
                    Color::DarkCyan,
                    2,
                    "{:02x}",
                    i - 256
                );
            } else if 512 <= i && i <= 767 {
                if mdec.decode((i - 512) as u8).is_some() {
                    // データが3バイト揃ったので表示
                    write_with_bg!(
                        shtctl,
                        sht_back,
                        buf_addr_back,
                        *SCREEN_WIDTH as isize,
                        *SCREEN_HEIGHT as isize,
                        32,
                        0,
                        Color::White,
                        Color::DarkCyan,
                        15,
                        "[{}{}{} {:>4} {:>4}]", 
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
                    );

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
            } else if i == 10 {
                write_with_bg!(
                    shtctl,
                    sht_back,
                    buf_addr_back,
                    *SCREEN_WIDTH as isize,
                    *SCREEN_HEIGHT as isize,
                    0,
                    64,
                    Color::White,
                    Color::DarkCyan,
                    7,
                    "10[sec]"
                );
                if !count_done {
                    write_with_bg!(
                        shtctl,
                        sht_win,
                        buf_addr_win,
                        160,
                        52,
                        40,
                        28,
                        Color::Black,
                        Color::LightGray,
                        10,
                        "{:>010}",
                        count
                    );
                    count_done = true;
                }
            } else if i == 3 {
                write_with_bg!(
                    shtctl,
                    sht_back,
                    buf_addr_back,
                    *SCREEN_WIDTH as isize,
                    *SCREEN_HEIGHT as isize,
                    0,
                    80,
                    Color::White,
                    Color::DarkCyan,
                    6,
                    "3[sec]"
                );
                count = 0;  // 測定開始
            } else {
                if i != 0 {
                    TIMER_MANAGER.lock().init_timer(timer_index3, 0);
                    boxfill(buf_addr_back, *SCREEN_WIDTH as isize, Color::White, 8, 96, 15, 111);
                } else {
                    TIMER_MANAGER.lock().init_timer(timer_index3, 1);
                    boxfill(buf_addr_back, *SCREEN_WIDTH as isize, Color::DarkCyan, 8, 96, 15, 111);
                }
                TIMER_MANAGER.lock().set_time(timer_index3, 50);
                shtctl.refresh(sht_back, 8, 96, 16, 112);
            }
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
