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
mod keyboard;
mod multi_task;
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
    use asm::{cli, sti, stihlt, load_tr, farjmp};
    use fifo::Fifo;
    use interrupt::{enable_mouse, init_keyboard};
    use memory::{MemMan, MEMMAN_ADDR};
    use mouse::{Mouse, MouseDec, MOUSE_CURSOR_HEIGHT, MOUSE_CURSOR_WIDTH};
    use sheet::SheetManager;
    use timer::TIMER_MANAGER;
    use vga::{
        boxfill, init_palette, init_screen, make_window, Color, ScreenWriter, SCREEN_HEIGHT,
        SCREEN_WIDTH,
    };
    use keyboard::KEYTABLE;
    use descriptor_table::{ADR_GDT, AR_TSS32, SegmentDescriptor};
    use multi_task::TSS;

    let fifo = Fifo::new(128);
    let fifo_addr = &fifo as *const Fifo as usize;

    descriptor_table::init();
    interrupt::init();
    sti();
    timer::init_pit();
    interrupt::allow_input();
    interrupt::init_keyboard(fifo_addr);
    vga::init_palette();
    enable_mouse(fifo_addr);

    let timer_ts = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_ts, fifo_addr, 2);
    TIMER_MANAGER.lock().set_time(timer_ts, 2);
    
    let timer_index1 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index1, fifo_addr, 10);
    TIMER_MANAGER.lock().set_time(timer_index1, 1000);
    let timer_index2 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index2, fifo_addr, 3);
    TIMER_MANAGER.lock().set_time(timer_index2, 300);
    let timer_index3 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index3, fifo_addr, 1);
    TIMER_MANAGER.lock().set_time(timer_index3, 50);
    
    let memtotal = memory::memtest(0x00400000, 0xbfffffff);
    let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
    *memman = MemMan::new();
    memman.free(0x00001000, 0x0009e000);   // 0x00001000 - 0x0009efff
    memman.free(0x00400000, 2);
    memman.free(0x00400000, memtotal  - 0x00400000);

    let shtctl_addr = memman
        .alloc_4k(core::mem::size_of::<SheetManager>() as u32)
        .unwrap();
    let shtctl = unsafe {
        &mut *(shtctl_addr as *mut SheetManager)
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

    vga::make_window(buf_addr_win, 160, 52, "counter");
    vga::make_textbox(buf_addr_win, 160, 8, 28, 144, 16, Color::White);

    shtctl.slide(sht_mouse, mx, my);
    shtctl.slide(sht_win, 80, 72);
    shtctl.updown(sht_back, Some(0));
    shtctl.updown(sht_win, Some(1));
    shtctl.updown(sht_mouse, Some(2));

    write_with_bg!(
        shtctl,
        sht_back,
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

    let mut tss_a: TSS = Default::default();
    tss_a.ldtr = 0;
    tss_a.iomap = 0x40000000;
    let mut tss_b: TSS = Default::default();
    tss_b.ldtr = 0;
    tss_b.iomap = 0x40000000;
    let gdt = unsafe { &mut *((ADR_GDT + 3 * 8) as *mut SegmentDescriptor) };
    *gdt = SegmentDescriptor::new(103, &tss_a as *const TSS as i32, AR_TSS32);
    let gdt = unsafe { &mut *((ADR_GDT + 4 * 8) as *mut SegmentDescriptor) };
    *gdt = SegmentDescriptor::new(103, &tss_b as *const TSS as i32, AR_TSS32);
    load_tr(3 * 8);
    let task_b_esp = memman.alloc_4k(64 * 1024).unwrap() + 64 * 1024 - 12;
    tss_b.eip = task_b_main as i32;
    tss_b.eflags = 0x00000202;  // IF = 1
    tss_b.eax = 0;
    tss_b.ecx = 0;
    tss_b.edx = 0;
    tss_b.ebx = 0;
    tss_b.esp = task_b_esp as i32;
    unsafe {
        *((task_b_esp + 4) as *mut usize) = shtctl_addr as usize;
        *((task_b_esp + 8) as *mut usize) = sht_back;
    }
    tss_b.ebp = 0;
    tss_b.esi = 0;
    tss_b.edi = 0;
    tss_b.es = 1 * 8;
    tss_b.cs = 2 * 8;
    tss_b.ss = 1 * 8;
    tss_b.ds = 1 * 8;
    tss_b.fs = 1 * 8;
    tss_b.gs = 1 * 8; 

    // let mut count = 0;
    // let mut count_done = false;

    let mut cursor_x = 8;
    let mut cursor_c = Color::White;
    loop {
        // count += 1;
        cli();
        
        if fifo.status() != 0 {
            let i = fifo.get().unwrap();
            sti();
            if i == 2 {
                farjmp(0, 4 * 8);
                TIMER_MANAGER.lock().set_time(timer_ts, 2);
            } else
            if 256 <= i && i <= 511 {   // キーボード
                write_with_bg!(
                    shtctl,
                    sht_back,
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
                if i < 256 + 0x54 {
                    let key = (i - 256) as usize;
                    if KEYTABLE[key] != 0 && cursor_x < 144 {
                        write_with_bg!(
                            shtctl,
                            sht_win,
                            160,
                            52,
                            cursor_x,
                            28,
                            Color::Black,
                            Color::White,
                            1,
                            "{}",
                            KEYTABLE[key] as char
                        );
                        cursor_x += 8;
                    }
                }
                if i == 256 + 0x0e && cursor_x > 8 {    // バックスペース
                    // カーソルをスペースで消してからカーソルを一つ戻す
                    write_with_bg!(
                        shtctl,
                        sht_win,
                        160,
                        52,
                        cursor_x,
                        28,
                        Color::Black,
                        Color::White,
                        1,
                        " ",
                    );
                    cursor_x -= 8;
                }
                // カーソルの再表示
                boxfill(buf_addr_win, 160, cursor_c, cursor_x, 28, cursor_x + 7, 43);
                shtctl.refresh(sht_win, cursor_x as i32, 28, cursor_x as i32 + 8, 44);
            } else if 512 <= i && i <= 767 {
                if mdec.decode((i - 512) as u8).is_some() {
                    // データが3バイト揃ったので表示
                    write_with_bg!(
                        shtctl,
                        sht_back,
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
                    if (mdec.btn.get() & 0x01) != 0 {
                        // 左クリックされていたらsht_winを動かす
                        shtctl.slide(sht_win, mx - 80, my - 8);
                    }
                }
            } else if i == 10 {
                write_with_bg!(
                    shtctl,
                    sht_back,
                    *SCREEN_WIDTH as isize,
                    *SCREEN_HEIGHT as isize,
                    0,
                    64,
                    Color::White,
                    Color::DarkCyan,
                    7,
                    "10[sec]"
                );
                // if !count_done {
                //     write_with_bg!(
                //         shtctl,
                //         sht_win,
                //         buf_addr_win,
                //         160,
                //         52,
                //         40,
                //         28,
                //         Color::Black,
                //         Color::LightGray,
                //         10,
                //         "{:>010}",
                //         count
                //     );
                //     count_done = true;
                // }
            } else if i == 3 {
                write_with_bg!(
                    shtctl,
                    sht_back,
                    *SCREEN_WIDTH as isize,
                    *SCREEN_HEIGHT as isize,
                    0,
                    80,
                    Color::White,
                    Color::DarkCyan,
                    6,
                    "3[sec]"
                );
                // count = 0;  // 測定開始
            } else {
                if i != 0 {
                    TIMER_MANAGER.lock().init_timer(timer_index3, fifo_addr, 0);
                    cursor_c = Color::Black;
                } else {
                    TIMER_MANAGER.lock().init_timer(timer_index3, fifo_addr, 1);
                    cursor_c = Color::White;
                }
                TIMER_MANAGER.lock().set_time(timer_index3, 50);
                boxfill(buf_addr_win, 160, cursor_c, cursor_x, 28, cursor_x + 8, 43);
                shtctl.refresh(sht_win, cursor_x as i32, 28, cursor_x as i32 + 8, 44);
            }
        } else {
            stihlt();
        }
    }
}

fn task_b_main(shtctl_addr: usize, sht_back: usize) {
    use asm::{cli, sti, stihlt, farjmp};
    use fifo::Fifo;
    use timer::TIMER_MANAGER;
    use vga::{Color, boxfill, ScreenWriter, SCREEN_HEIGHT, SCREEN_WIDTH};
    use sheet::SheetManager;

    let fifo = Fifo::new(128);
    let fifo_addr = &fifo as *const Fifo as usize;

    let shtctl = unsafe {
        &mut *(shtctl_addr as *mut SheetManager)
    };

    let timer_ts = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_ts, fifo_addr, 2);
    TIMER_MANAGER.lock().set_time(timer_ts, 2);

    let timer_put = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_put, fifo_addr, 1);
    TIMER_MANAGER.lock().set_time(timer_put, 1);

    let timer_1s = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_1s, fifo_addr, 100);
    TIMER_MANAGER.lock().set_time(timer_1s, 100);

    let mut count = 0;
    let mut count0 = 0;
    loop {
        count += 1;
        cli();
        if fifo.status() == 0 {
            sti();
        } else {
            let i = fifo.get().unwrap();
            sti();
            if i == 1 {
                write_with_bg!(
                    shtctl,
                    sht_back,
                    *SCREEN_WIDTH as isize,
                    *SCREEN_HEIGHT as isize,
                    10,
                    144,
                    Color::White,
                    Color::DarkCyan,
                    11,
                    "{:>11}",
                    count
                );
                TIMER_MANAGER.lock().set_time(timer_put, 1);
            } else if i == 2 {
                farjmp(0, 3 * 8);
                TIMER_MANAGER.lock().set_time(timer_ts, 2);
            } else if i == 100 {
                write_with_bg!(
                    shtctl,
                    sht_back,
                    *SCREEN_WIDTH as isize,
                    *SCREEN_HEIGHT as isize,
                    0,
                    128,
                    Color::White,
                    Color::DarkCyan,
                    11,
                    "{:>11}",
                    count - count0
                );
                count0 = count;
                TIMER_MANAGER.lock().set_time(timer_1s, 100);
            }
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
