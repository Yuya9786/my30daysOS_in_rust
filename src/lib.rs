#![no_std]
#![feature(asm)]
#![feature(start)]
#![feature(naked_functions)]

use core::fmt::Write;
use core::panic::PanicInfo;
use core::str::from_utf8;

use asm::{cli, out8, sti};
use fifo::Fifo;
use interrupt::PORT_KEYDAT;
use keyboard::{wait_kbc_sendready, KEYBOARD_OFFSET, KEYCMD_LED, KEYTABLE0, KEYTABLE1, LOCK_KEYS};
use memory::{MemMan, MEMMAN_ADDR};
use mouse::{Mouse, MouseDec, MOUSE_CURSOR_HEIGHT, MOUSE_CURSOR_WIDTH};
use multi_task::{TaskManager, TASK_MANAGER_ADDR};
use sheet::{SheetManager, Sheet};
use timer::TIMER_MANAGER;
use vga::{
    boxfill, init_palette, init_screen, make_textbox, make_window, make_wtitle, Color,
    ScreenWriter, SCREEN_HEIGHT, SCREEN_WIDTH,
};
use file::{FileInfo, ADR_DISKIMG, ADR_FILE_OFFSET, MAX_FILE_INFO};

mod asm;
mod descriptor_table;
mod fifo;
mod fonts;
mod interrupt;
mod keyboard;
mod memory;
mod mouse;
mod multi_task;
mod sheet;
mod timer;
mod vga;
mod file;

static mut SHEET_MANAGER_ADDR: usize = 0;
const CONSOLE_CURSOR_ON: u32 = 2;
const CONSOLE_CURSOR_OFF: u32 = 3;
const CONSOLE_ENTER: u32 = 10;

#[no_mangle]
#[start]
pub extern "C" fn HariMain() {
    descriptor_table::init();
    interrupt::init();
    sti();
    interrupt::allow_input();

    let mut fifo = &mut Fifo::new(128, None);
    let fifo_addr = fifo as *const Fifo as usize;

    keyboard::init_keyboard(fifo_addr);
    timer::init_pit();
    init_palette();
    mouse::enable_mouse(fifo_addr);

    let memtotal = memory::memtest(0x00400000, 0xbfffffff);
    let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
    *memman = MemMan::new();
    memman.free(0x00001000, 0x0009e000).unwrap();
    memman.free(0x00400000, 2).unwrap();
    memman.free(0x00400000, memtotal - 0x00400000).unwrap();

    let timer_index3 = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index3, fifo_addr, 1);
    TIMER_MANAGER.lock().set_time(timer_index3, 50);

    let task_manager_addr = memman
        .alloc_4k(core::mem::size_of::<TaskManager>() as u32)
        .unwrap();
    unsafe {
        TASK_MANAGER_ADDR = task_manager_addr as usize;
    }
    let task_manager = unsafe { &mut *(task_manager_addr as *mut TaskManager) };
    *task_manager = TaskManager::new();
    let task_a_index = task_manager.init(memman, fifo_addr).unwrap();
    fifo.task_index = Some(task_a_index);

    let sheet_manager_addr = memman
        .alloc_4k(core::mem::size_of::<SheetManager>() as u32)
        .unwrap();
    let sheet_manager = unsafe { &mut *(sheet_manager_addr as *mut SheetManager) };
    let sheet_map_addr = memman
        .alloc_4k(*SCREEN_HEIGHT as u32 * *SCREEN_WIDTH as u32)
        .unwrap();
    *sheet_manager = SheetManager::new(sheet_map_addr as i32);
    let shi_bg = sheet_manager.alloc().unwrap();
    unsafe {
        SHEET_MANAGER_ADDR = sheet_manager_addr as usize;
    }
    let shi_mouse = sheet_manager.alloc().unwrap();
    let shi_win = sheet_manager.alloc().unwrap();
    let scrnx = *SCREEN_WIDTH as i32;
    let scrny = *SCREEN_HEIGHT as i32;
    let buf_bg_addr = memman.alloc_4k((scrnx * scrny) as u32).unwrap() as usize;
    let buf_win_addr = memman.alloc_4k((144 * 52) as u32).unwrap() as usize;
    let buf_mouse = [0u8; MOUSE_CURSOR_WIDTH * MOUSE_CURSOR_HEIGHT];
    let buf_mouse_addr =
        &buf_mouse as *const [u8; MOUSE_CURSOR_HEIGHT * MOUSE_CURSOR_WIDTH] as usize;
    sheet_manager.set_buf(shi_bg, buf_bg_addr, scrnx, scrny, None);
    sheet_manager.set_buf(shi_win, buf_win_addr, 144, 52, None);
    sheet_manager.set_buf(
        shi_mouse,
        buf_mouse_addr,
        MOUSE_CURSOR_WIDTH as i32,
        MOUSE_CURSOR_HEIGHT as i32,
        Some(Color::DarkCyan),
    );

    init_screen(buf_bg_addr);
    let mouse_dec = MouseDec::new();
    let mx = (scrnx as i32 - MOUSE_CURSOR_WIDTH as i32) / 2;
    let my = (scrny as i32 - MOUSE_CURSOR_HEIGHT as i32 - 28) / 2;
    let mouse = Mouse::new(buf_mouse_addr);
    mouse.render();

    make_window(buf_win_addr, 144, 52, "task_a", true);
    make_textbox(buf_win_addr, 144, 8, 28, 128, 16, Color::White);

    task_manager.run(task_a_index, 1, 2);

    const CONSOLE_WIDTH: usize = 256;
    const CONSOLE_HEIGHT: usize = 165;

    let shi_console = sheet_manager.alloc().unwrap();
    let buf_console = memman
        .alloc_4k((CONSOLE_WIDTH * CONSOLE_HEIGHT) as u32)
        .unwrap() as usize;
    sheet_manager.set_buf(
        shi_console,
        buf_console,
        CONSOLE_WIDTH as i32,
        CONSOLE_HEIGHT as i32,
        None,
    );
    make_window(
        buf_console,
        CONSOLE_WIDTH as isize,
        CONSOLE_HEIGHT as isize,
        "console",
        false,
    );
    make_textbox(
        buf_console,
        CONSOLE_WIDTH as isize,
        8,
        28,
        240,
        128,
        Color::Black,
    );
    let console_task_index = task_manager.alloc().unwrap();
    let mut console_task_mut = &mut task_manager.tasks_data[console_task_index];
    let console_esp = memman.alloc_4k(64 * 1024).unwrap() + 64 * 1024 - 12;
    console_task_mut.tss.esp = console_esp as i32;
    console_task_mut.tss.eip = console_task as i32;
    console_task_mut.tss.es = 1 * 8;
    console_task_mut.tss.cs = 2 * 8;
    console_task_mut.tss.ss = 1 * 8;
    console_task_mut.tss.ds = 1 * 8;
    console_task_mut.tss.fs = 1 * 8;
    console_task_mut.tss.gs = 1 * 8;
    let ptr = unsafe { &mut *((console_task_mut.tss.esp + 4) as *mut usize) };
    *ptr = shi_console;
    let ptr = unsafe { &mut *((console_task_mut.tss.esp + 8) as *mut u32) };
    *ptr = memtotal;
    task_manager.run(console_task_index, 2, 2);

    sheet_manager.slide(shi_mouse, mx, my);
    sheet_manager.slide(shi_console, 32, 4);
    sheet_manager.slide(shi_win, 64, 56);
    sheet_manager.updown(shi_bg, Some(0));
    sheet_manager.updown(shi_console, Some(1));
    sheet_manager.updown(shi_win, Some(2));
    sheet_manager.updown(shi_mouse, Some(3));

    // カーソル
    let min_cursor_x = 8;
    let max_cursor_x = 144;
    let mut cursor_x = min_cursor_x;
    let mut cursor_c = Color::White;

    let mut active_window: usize = 0;

    // シフトキー
    let mut key_shift = (false, false);
    // CapsLock, NumLock, ScreenLock
    let mut lock_keys = *LOCK_KEYS;
    let mut keycmd_wait: i32 = -1;
    // キーボードの状態管理用のFifo
    let keycmd = Fifo::new(32, None);
    keycmd.put(KEYCMD_LED as u32).unwrap();
    keycmd.put(lock_keys.as_bytes() as u32).unwrap();

    let mut cursor_on = true;    // カーソルを点滅するかどうか

    loop {
        // キーボードコントローラに送ルデータがあれば送る
        if keycmd.status() > 0 && keycmd_wait < 0 {
            keycmd_wait = keycmd.get().unwrap() as u8 as i32;
            wait_kbc_sendready();
            out8(PORT_KEYDAT, keycmd_wait as u8);
        }
        cli();
        if fifo.status() != 0 {
            let i = fifo.get().unwrap();
            sti();
            if KEYBOARD_OFFSET <= i && i <= 511 {
                let key = i - KEYBOARD_OFFSET;
                let mut chr = 0 as u8;
                if key < KEYTABLE0.len() as u32 {
                    if key_shift == (false, false) {
                        chr = KEYTABLE0[key as usize];
                    } else {
                        chr = KEYTABLE1[key as usize];
                    }
                }
                if b'A' <= chr && chr <= b'Z' {
                    // アルファベットの場合、ShiftキーとCapsLockの状態で大文字小文字を決める
                    if !lock_keys.caps_lock && key_shift == (false, false)
                        || lock_keys.caps_lock && key_shift != (false, false)
                    {
                        chr += 0x20;
                    }
                }
                if chr != 0 {
                    if active_window == 0 {
                        if cursor_x < max_cursor_x {
                            write_with_bg!(
                                sheet_manager,
                                shi_win,
                                144,
                                52,
                                cursor_x,
                                28,
                                Color::Black,
                                Color::White,
                                1,
                                "{}",
                                chr as char,
                            );
                            cursor_x += 8;
                        }
                    } else {
                        let ctask = task_manager.tasks_data[console_task_index];
                        let fifo = unsafe { &*(ctask.fifo_addr as *const Fifo) };
                        fifo.put(chr as u32 + KEYBOARD_OFFSET).unwrap();
                    }
                }
                // バックスペース
                if key == 0x0e && cursor_x > min_cursor_x {
                    if active_window == 0 {
                        write_with_bg!(
                            sheet_manager,
                            shi_win,
                            144,
                            52,
                            cursor_x,
                            28,
                            Color::Black,
                            Color::White,
                            1,
                            " "
                        );
                        cursor_x -= 8;
                    } else {
                        let ctask = task_manager.tasks_data[console_task_index];
                        let fifo = unsafe { &*(ctask.fifo_addr as *const Fifo) };
                        fifo.put(chr as u32 + KEYBOARD_OFFSET).unwrap();
                    }
                }
                // タブ
                if key == 0x0f {
                    let sheet_win = sheet_manager.sheets_data[shi_win];
                    let sheet_console = sheet_manager.sheets_data[shi_console];
                    if active_window == 0 {
                        active_window = 1;
                        cursor_on = false;  // カーソルを消す
                        make_wtitle(
                            sheet_win.buf_addr,
                            sheet_win.width as isize,
                            sheet_win.height as isize,
                            "task_a",
                            false,
                        );
                        make_wtitle(
                            sheet_console.buf_addr,
                            sheet_console.width as isize,
                            sheet_console.height as isize,
                            "console",
                            true,
                        );
                        cursor_c = Color::White;
                        let ctask = task_manager.tasks_data[console_task_index];
                        let fifo = unsafe { &*(ctask.fifo_addr as *const Fifo) };
                        fifo.put(CONSOLE_CURSOR_ON).unwrap();
                    } else {
                        active_window = 0;
                        cursor_on = true;  // カーソルを表示
                        make_wtitle(
                            sheet_win.buf_addr,
                            sheet_win.width as isize,
                            sheet_win.height as isize,
                            "task_a",
                            true,
                        );
                        make_wtitle(
                            sheet_console.buf_addr,
                            sheet_console.width as isize,
                            sheet_console.height as isize,
                            "console",
                            false,
                        );
                        cursor_c = Color::Black;
                        let ctask = task_manager.tasks_data[console_task_index];
                        let fifo = unsafe { &*(ctask.fifo_addr as *const Fifo) };
                        fifo.put(CONSOLE_CURSOR_OFF).unwrap();
                    }
                    sheet_manager.refresh(shi_win, 0, 0, sheet_win.width, 21);
                    sheet_manager.refresh(shi_console, 0, 0, sheet_console.width, 21);
                }
                // 左シフト ON
                if key == 0x2a {
                    key_shift.0 = true;
                }
                // 右シフト ON
                if key == 0x36 {
                    key_shift.1 = true;
                }
                // 左シフト OFF
                if key == 0xaa {
                    key_shift.0 = false;
                }
                // 右シフト OFF
                if key == 0xb6 {
                    key_shift.1 = false;
                }
                // CapsLock
                if key == 0x3a {
                    lock_keys.caps_lock = !lock_keys.caps_lock;
                    keycmd.put(KEYCMD_LED as u32).unwrap();
                    keycmd.put(lock_keys.as_bytes() as u32).unwrap();
                }
                // NumLock
                if key == 0x45 {
                    lock_keys.num_lock = !lock_keys.num_lock;
                    keycmd.put(KEYCMD_LED as u32).unwrap();
                    keycmd.put(lock_keys.as_bytes() as u32).unwrap();
                }
                // ScrollLock
                if key == 0x46 {
                    lock_keys.scroll_lock = !lock_keys.scroll_lock;
                    keycmd.put(KEYCMD_LED as u32).unwrap();
                    keycmd.put(lock_keys.as_bytes() as u32).unwrap();
                }
                // Enter
                if key == 0x1c {
                    if active_window != 0 {
                        let ctask = task_manager.tasks_data[console_task_index];
                        let fifo = unsafe { &*(ctask.fifo_addr as *const Fifo) };
                        fifo.put(CONSOLE_ENTER + KEYBOARD_OFFSET).unwrap();
                    }
                }
                // キーボードがデータを無事に受け取った
                if key == 0xfa {
                    keycmd_wait = -1;
                }
                // キーボードがデータを無事に受け取れなかった
                if key == 0xfe {
                    wait_kbc_sendready();
                    out8(PORT_KEYDAT, keycmd_wait as u8);
                }
                if cursor_on {
                    boxfill(buf_win_addr, 144, cursor_c, cursor_x, 28, cursor_x + 8, 43);
                }
                sheet_manager.refresh(shi_win, cursor_x as i32, 28, cursor_x as i32 + 8, 44)
            } else if 512 <= i && i <= 767 {
                if mouse_dec.decode((i - 512) as u8).is_some() {
                    let (new_x, new_y) = sheet_manager.get_new_point(
                        shi_mouse,
                        mouse_dec.x.get(),
                        mouse_dec.y.get(),
                    );
                    sheet_manager.slide(shi_mouse, new_x, new_y);
                    // 左クリックをおしていた場合
                    if (mouse_dec.btn.get() & 0x01) != 0 {
                        sheet_manager.slide(shi_win, new_x - 80, new_y - 8);
                    }
                }
            } else {
                if i != 0 {
                    TIMER_MANAGER.lock().init_timer(timer_index3, fifo_addr, 0);
                    cursor_c = if cursor_on {
                        Color::Black
                    } else {
                        Color::White
                    };
                } else {
                    TIMER_MANAGER.lock().init_timer(timer_index3, fifo_addr, 1);
                    cursor_c = Color::White;
                }
                TIMER_MANAGER.lock().set_time(timer_index3, 50);
                if cursor_on {
                    boxfill(buf_win_addr, 144, cursor_c, cursor_x, 28, cursor_x + 8, 43);
                    sheet_manager.refresh(shi_win, cursor_x as i32, 28, cursor_x as i32 + 8, 44)
                }
            }
        } else {
            task_manager.sleep(task_a_index);
            sti();
        }
    }
}

pub extern "C" fn console_task(sheet_index: usize, memtotal: u32) {
    let task_manager = unsafe { &mut *(TASK_MANAGER_ADDR as *mut TaskManager) };
    let task_index = task_manager.now_index();

    let fifo = Fifo::new(128, Some(task_index));
    let fifo_addr = &fifo as *const Fifo as usize;
    {
        let mut task = &mut task_manager.tasks_data[task_index];
        task.fifo_addr = fifo_addr;
    }

    let mut cursor_x: isize = 16;
    let mut cursor_y: isize = 28;
    let mut cursor_c = Color::Black;
    let min_cursor_x = 16;
    let min_cursor_y = 28;
    let max_cursor_x = 240;
    let max_cursor_y = 140;

    let mut cmdline: [u8; 30] = [0; 30];

    let sheet_manager_addr = unsafe { SHEET_MANAGER_ADDR };
    let sheet_manager = unsafe { &mut *(sheet_manager_addr as *mut SheetManager) };
    let sheet = sheet_manager.sheets_data[sheet_index];

    let timer_index = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index, fifo_addr, 1);
    TIMER_MANAGER.lock().set_time(timer_index, 50);

    macro_rules! display_error {
        ($error: tt, $cursor_y: tt) => {
            write_with_bg!(
                sheet_manager,
                sheet_index,
                sheet.width,
                sheet.height,
                8,
                $cursor_y,
                Color::White,
                Color::Black,
                30,
                $error
            );
            cursor_y = cons_newline($cursor_y, sheet_manager, sheet_index);
            cursor_y = cons_newline($cursor_y, sheet_manager, sheet_index);
        };
    }
    
    // プロンプト表示
    write_with_bg!(
        sheet_manager,
        sheet_index,
        sheet.width,
        sheet.height,
        8,
        28,
        Color::White,
        Color::Black,
        1,
        ">"
    );

    let mut cursor_on = false;

    loop {
        cli();
        if fifo.status() == 0 {
            task_manager.sleep(task_index);
            sti();
        } else {
            let i = fifo.get().unwrap();
            sti();
            if i <= 1 {
                if i != 0 {
                    TIMER_MANAGER.lock().init_timer(timer_index, fifo_addr, 0);
                    cursor_c = if cursor_on {
                        Color::White
                    } else {
                        Color::Black
                    };
                } else {
                    TIMER_MANAGER.lock().init_timer(timer_index, fifo_addr, 1);
                    cursor_c = Color::Black;
                }
                TIMER_MANAGER.lock().set_time(timer_index, 50);
            } else if i == 2 {
                cursor_on = true;
            } else if i == 3 {
                cursor_on = false;
            } else if KEYBOARD_OFFSET <= i && i <= 511 {
                let key = (i - KEYBOARD_OFFSET) as u8;
                if key != 0 {
                    if key == 0x0e {
                        if cursor_x > min_cursor_x {
                            write_with_bg!(
                                sheet_manager,
                                sheet_index,
                                sheet.width,
                                sheet.height,
                                cursor_x,
                                cursor_y,
                                Color::White,
                                Color::Black,
                                1,
                                " "
                            );
                            cursor_x -= 8;
                        }
                    } else if key == CONSOLE_ENTER as u8 {
                        // Enter
                        // カーソルをスペースで消す
                        write_with_bg!(
                            sheet_manager,
                            sheet_index,
                            sheet.width,
                            sheet.height,
                            cursor_x,
                            cursor_y,
                            Color::White,
                            Color::Black,
                            1,
                            " "
                        );
                        let cmd_end = cursor_x as usize / 8 - 2;
                        cmdline[cmd_end] = 0;
                        cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                        let cmdline_strs = cmdline.split(|s| *s == 0 || *s == b' ');
                        let mut cmdline_strs = cmdline_strs.skip_while(|cmd| cmd.len() == 0);
                        let cmd = cmdline_strs.next();
                        let cmd = from_utf8(&cmd.unwrap()).unwrap();

                        // コマンド実行
                        match cmd {
                            "mem" => {
                                // memコマンド
                                let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
                                write_with_bg!(
                                    sheet_manager,
                                    sheet_index,
                                    sheet.width,
                                    sheet.height,
                                    8,
                                    cursor_y,
                                    Color::White,
                                    Color::Black,
                                    30,
                                    "total   {}MB",
                                    memtotal / (1024 * 1024)
                                );
                                cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                write_with_bg!(
                                    sheet_manager,
                                    sheet_index,
                                    sheet.width,
                                    sheet.height,
                                    8,
                                    cursor_y,
                                    Color::White,
                                    Color::Black,
                                    30,
                                    "free {}KB",
                                    memman.total() / 1024
                                );
                                cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                            },
                            "clear" => {
                                for y in 28..28+128 {
                                    for x in 8..8+240 {
                                        let ptr = unsafe {
                                            &mut *((sheet.buf_addr + x + y * sheet.width as usize) as *mut u8)
                                        };
                                        *ptr = Color::Black as u8;
                                    }
                                    sheet_manager.refresh(
                                        sheet_index, 8, 28, 8+240, 28+128
                                    );
                                    cursor_y = 28;
                                }
                            },
                            "ls" => {
                                for x in 0..MAX_FILE_INFO {
                                    let finfo = unsafe {
                                        *((ADR_DISKIMG + ADR_FILE_OFFSET + x * core::mem::size_of::<FileInfo>()) as *const FileInfo)
                                    };
                                    if finfo.name[0] == 0x00 {
                                        break;
                                    }
                                    if finfo.name[0] != 0xe5 {
                                        if (finfo.ftype & 0x18) == 0 {
                                            write_with_bg!(
                                                sheet_manager,
                                                sheet_index,
                                                sheet.width,
                                                sheet.height,
                                                8,
                                                cursor_y,
                                                Color::White,
                                                Color::Black,
                                                30,
                                                "{:>8}.{:>3}     {:>7}",
                                                from_utf8(&finfo.name).unwrap(),
                                                from_utf8(&finfo.ext).unwrap(),
                                                finfo.size
                                            );
                                            cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                        }
                                    }
                                }
                                cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                            },
                            "cat" => {
                                let mut filename = cmdline_strs.next();
                                if filename.is_none() {
                                    display_error!("File not found", cursor_y);
                                } else {
                                    let filename = filename.unwrap();
                                    let mut filename = filename.split(|c| *c == b'.');
                                    let basename = filename.next();
                                    let extname = filename.next();
                                    let mut b = [b' '; 8];
                                    let mut e = [b' '; 3];
                                    if let Some(basename) = basename {
                                        for x in 0..basename.len() {
                                            if b'a' <= basename[x] && basename[x] <= b'z' {
                                                // 小文字は大文字に直す
                                                b[x] = basename[x] - 0x20;
                                            } else {
                                                b[x] = basename[x];
                                            }
                                        }
                                        if let Some(extname) = extname {
                                            for x in 0..extname.len() {
                                                if b'a' <= extname[x] && extname[x] <= b'z' {
                                                    e[x] = extname[x] - 0x20;
                                                } else {
                                                    e[x] = extname[x];
                                                }
                                            }
                                        }
                                        let mut target_finfo: Option<FileInfo> = None;
                                        for x in 0..MAX_FILE_INFO {
                                            let finfo = unsafe {
                                                *((ADR_DISKIMG + ADR_FILE_OFFSET + x * core::mem::size_of::<FileInfo>()) as *const FileInfo)
                                            };
                                            if finfo.name[0] == 0x00 {
                                                break;
                                            }
                                            if (finfo.ftype & 0x18) == 0 {
                                                let mut filename_equal = true;
                                                for y in 0..finfo.name.len() {
                                                    if finfo.name[y] != b[y] {
                                                        filename_equal = false;
                                                        break;
                                                    }
                                                }
                                                for y in 0..finfo.ext.len() {
                                                    if finfo.ext[y] != e[y] {
                                                        filename_equal = false;
                                                        break;
                                                    }
                                                }
                                                if filename_equal {
                                                    // ファイルが見つかった
                                                    target_finfo = Some(finfo);
                                                    break;
                                                }
                                            }
                                        }
                                        if let Some(finfo) = target_finfo {
                                            // ファイルが見つかった場合
                                            let content_length = finfo.size;
                                            let mut cursor_x = 8;
                                            for c in 0..content_length {
                                                let p = unsafe { *((finfo.clustno as usize * 512 + 0x003e00 + ADR_DISKIMG + c as usize) as *const u8) };
                                                if p == 0x09 {
                                                    // タブ
                                                    loop {
                                                        write_with_bg!(
                                                            sheet_manager,
                                                            sheet_index,
                                                            sheet.width,
                                                            sheet.height,
                                                            cursor_x,
                                                            cursor_y,
                                                            Color::White,
                                                            Color::Black,
                                                            1,
                                                            " ",
                                                        );
                                                        cursor_x += 8;
                                                        if cursor_x == 8 + 240 {
                                                            cursor_x = 8;
                                                            cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                                        }
                                                        if ((cursor_x - 8) & 0x1f) == 0 {
                                                            // 32で割り切れたら
                                                            break;
                                                        }
                                                    }
                                                } else if p == 0x0a {
                                                    // 改行
                                                    cursor_x = 8;
                                                    cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                                } else if p == 0x0d {
                                                    // 復帰（とりあえず何もしない）
                                                } else {
                                                    // 普通の文字
                                                    write_with_bg!(
                                                        sheet_manager,
                                                        sheet_index,
                                                        sheet.width,
                                                        sheet.height,
                                                        cursor_x,
                                                        cursor_y,
                                                        Color::White,
                                                        Color::Black,
                                                        1,
                                                        "{}",
                                                        p as char
                                                    );
                                                    cursor_x += 8;
                                                    if cursor_x == 8 + 240 {
                                                        // 右端まで来たので改行
                                                        cursor_x = 8;
                                                        cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                                    }
                                                }
                                            }
                                            cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                        } else {
                                            display_error!("File not found", cursor_y);
                                        }
                                    } else {
                                        display_error!("File not found", cursor_y);
                                    }
                                }
                            },
                            _ => {
                                write_with_bg!(
                                    sheet_manager,
                                    sheet_index,
                                    sheet.width,
                                    sheet.height,
                                    8,
                                    cursor_y,
                                    Color::White,
                                    Color::Black,
                                    22,
                                    "Unknown Command: {}",
                                    cmd
                                );
                                cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                                cursor_y = cons_newline(cursor_y, sheet_manager, sheet_index);
                            },
                        }
                        
                        // プロンプト表示
                        write_with_bg!(
                            sheet_manager,
                            sheet_index,
                            sheet.width,
                            sheet.height,
                            8,
                            cursor_y,
                            Color::White,
                            Color::Black,
                            1,
                            ">"
                        );
                        cursor_x = 16;
                    } else {
                        if cursor_x < max_cursor_x {
                            cmdline[cursor_x as usize / 8 - 2] = key;
                            write_with_bg!(
                                sheet_manager,
                                sheet_index,
                                sheet.width,
                                sheet.height,
                                cursor_x,
                                cursor_y,
                                Color::White,
                                Color::Black,
                                1,
                                "{}",
                                key as char,
                            );
                            cursor_x += 8;
                        }
                    }
                }
            }
            if cursor_on {
                boxfill(
                    sheet.buf_addr,
                    sheet.width as isize,
                    cursor_c,
                    cursor_x,
                    cursor_y,
                    cursor_x + 7,
                    cursor_y + 15,
                );
            }
            sheet_manager.refresh(sheet_index, cursor_x as i32, cursor_y as i32, cursor_x as i32 + 8, cursor_y as i32 + 16);
        }
    }
}

fn cons_newline(cursor_y: isize, sheet_manager: &SheetManager, sheet_index: usize) -> isize {
    let sheet = sheet_manager.sheets_data[sheet_index];

    let min_cursor_x = 8;
    let max_cursor_x = 248;
    let min_cursor_y = 28;
    let max_cursor_y = 140;
    let mut cursor_y = cursor_y;
    if cursor_y < max_cursor_y {
        cursor_y += 16; // 次の行へ
    } else {
        // スクロール
        for y in min_cursor_y..max_cursor_y {
            for x in 8..(max_cursor_x + 8) {
                let x = x as usize;
                let y = y as usize;
                // スクロール
                let ptr = unsafe {
                    &mut *((sheet.buf_addr + x + y * sheet.width as usize) as *mut u8)
                };
                *ptr = unsafe {
                    *((sheet.buf_addr + x + (y + 16) * sheet.width as usize) as *const u8)
                }
            }
        }
        for y in max_cursor_y..(max_cursor_y + 16) {
            for x in 8..(max_cursor_x + 8) {
                let x = x as usize;
                let y = y as usize;
                // スクロール
                let ptr = unsafe {
                    &mut *((sheet.buf_addr + x + y * sheet.width as usize) as *mut u8)
                };
                *ptr = Color::Black as u8;
            }
        }
        sheet_manager.refresh(sheet_index, 8, min_cursor_y as i32, max_cursor_x as i32 + 8, max_cursor_y as i32 + 16);
    }

    cursor_y
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    let mut writer = ScreenWriter::new(
        None,
        vga::Color::LightRed,
        0,
        0,
        *SCREEN_WIDTH as usize,
        *SCREEN_HEIGHT as usize,
    );
    write!(writer, "[ERR] {:?}", info).unwrap();
    loop {
        asm::hlt()
    }
}
