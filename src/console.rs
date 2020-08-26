use core::fmt::Write;
use core::panic::PanicInfo;
use core::str::from_utf8;

use crate::asm::{cli, out8, sti, farjmp};
use crate::descriptor_table::{SegmentDescriptor, ADR_GDT, AR_CODE32_ER};
use crate::fifo::Fifo;
use crate::interrupt::PORT_KEYDAT;
use crate::keyboard::{wait_kbc_sendready, KEYBOARD_OFFSET, KEYCMD_LED, KEYTABLE0, KEYTABLE1, LOCK_KEYS};
use crate::memory::{MemMan, MEMMAN_ADDR};
use crate::mouse::{Mouse, MouseDec, MOUSE_CURSOR_HEIGHT, MOUSE_CURSOR_WIDTH};
use crate::multi_task::{TaskManager, TASK_MANAGER_ADDR};
use crate::sheet::{SheetManager, Sheet};
use crate::timer::TIMER_MANAGER;
use crate::vga::{
    boxfill, init_palette, init_screen, make_textbox, make_window, make_wtitle, Color,
    ScreenWriter, SCREEN_HEIGHT, SCREEN_WIDTH,
};
use crate::file::{FileInfo, ADR_DISKIMG, ADR_FILE_OFFSET, MAX_FILE_INFO, file_readfat};
use crate::{write_with_bg, SHEET_MANAGER_ADDR, CONSOLE_ENTER};

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

    let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };

    let fat_addr = memman.alloc_4k(4 * 2880).unwrap();
    let fat = unsafe { &mut *(fat_addr as *mut [u32; 2880]) };
    file_readfat(fat, unsafe { *((ADR_DISKIMG + 0x000200) as *const [u8; 2880 * 4]) });

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
                                    let target_finfo = search_file(filename);
                                    if let Some(finfo) = target_finfo {
                                        // ファイルが見つかった場合
                                        let content_addr = memman.alloc_4k(finfo.size).unwrap() as usize;
                                        finfo.file_loadfile(content_addr, fat, ADR_DISKIMG + 0x003e00);
                                        let mut cursor_x = 8;
                                        for c in 0..finfo.size {
                                            let p = unsafe { *((content_addr + c as usize) as *const u8) };
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
                                        memman.free_4k(content_addr as u32, finfo.size).unwrap();
                                    } else {
                                        display_error!("File not found", cursor_y);
                                    }
                                }
                            },
                            "hlt" => {
                                let target_finfo = search_file(b"hlt.bin");
                                if let Some(finfo) = target_finfo {
                                    let content_addr = memman.alloc_4k(finfo.size).unwrap() as usize;
                                    finfo.file_loadfile(content_addr, fat, ADR_DISKIMG + 0x003e00);
                                    let gdt = unsafe { &mut *((ADR_GDT + 1003 * 8) as *mut SegmentDescriptor) };
                                    *gdt = SegmentDescriptor::new(finfo.size - 1, content_addr as i32, AR_CODE32_ER);
                                    farjmp(0, 1003 * 8);
                                    memman.free_4k(content_addr as u32, finfo.size).unwrap();
                                } else {
                                    display_error!("File not found", cursor_y);
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

fn search_file(filename: &[u8]) -> Option<FileInfo> {
    let mut filename = filename.split(|c| *c == b'.');
    let basename = filename.next();
    let extname = filename.next();
    let mut b = [b' '; 8];
    let mut e = [b' '; 3];
    let mut target_finfo: Option<FileInfo> = None;
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
    }

    target_finfo
}
