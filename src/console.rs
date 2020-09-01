use core::fmt::Write;
use core::panic::PanicInfo;
use core::str::from_utf8;

use crate::asm::{cli, out8, sti, farcall};
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
use crate::file::{FileInfo, ADR_DISKIMG, ADR_FILE_OFFSET, MAX_FILE_INFO, MAX_FAT, file_readfat};
use crate::{write_with_bg, SHEET_MANAGER_ADDR, CONSOLE_ENTER, CONSOLE_BACKSPACE};

pub const MIN_CURSOR_X: isize = 16;
pub const MIN_CURSOR_Y: isize = 28;
pub const MAX_CURSOR_X: isize = 240;
pub const MAX_CURSOR_Y: isize = 140;
pub const CONSOLE_ADDR: usize = 0xfec;
pub const CS_BASE_ADDR: usize = 0xfe8;

pub extern "C" fn console_task(sheet_index: usize, memtotal: u32) {
    let task_manager = unsafe { &mut *(TASK_MANAGER_ADDR as *mut TaskManager) };
    let task_index = task_manager.now_index();

    let fifo = Fifo::new(128, Some(task_index));
    let fifo_addr = &fifo as *const Fifo as usize;
    {
        let mut task = &mut task_manager.tasks_data[task_index];
        task.fifo_addr = fifo_addr;
    }

    let sheet_manager_addr = unsafe { SHEET_MANAGER_ADDR };
    let sheet_manager = unsafe { &mut *(sheet_manager_addr as *mut SheetManager) };
    let sheet = sheet_manager.sheets_data[sheet_index];

    let mut console = Console::new(sheet_index, sheet_manager_addr);
    {
        let ptr = unsafe { &mut *(0x0fec as *mut usize) };
        *ptr = &console as *const Console as usize;
    }

    let timer_index = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().init_timer(timer_index, fifo_addr, 1);
    TIMER_MANAGER.lock().set_time(timer_index, 50);

    let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };

    let fat_addr = memman.alloc_4k(4 * 2880).unwrap();
    let fat = unsafe { &mut *(fat_addr as *mut [u32; 2880]) };
    file_readfat(fat, unsafe { *((ADR_DISKIMG + 0x000200) as *const [u8; 2880 * 4]) });
    
    // プロンプト表示
    console.put_chr(b'>', true);

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
                    console.cursor_c = if console.cursor_on {
                        Color::White
                    } else {
                        Color::Black
                    };
                } else {
                    TIMER_MANAGER.lock().init_timer(timer_index, fifo_addr, 1);
                    console.cursor_c = Color::Black;
                }
                TIMER_MANAGER.lock().set_time(timer_index, 50);
            } else if i == 2 {
                console.cursor_on = true;
            } else if i == 3 {
                console.cursor_on = false;
            } else if KEYBOARD_OFFSET <= i && i <= 511 {
                let key = (i - KEYBOARD_OFFSET) as u8;
                if key != 0 {
                    if key == 0x0e {
                        if console.cursor_x > MIN_CURSOR_X + 8 {
                            console.put_chr(b' ', false);
                            console.cmdline[console.cursor_x as usize / 8 - 2] = b' ';
                            console.cursor_x -= 8;
                        }
                    } else if key == CONSOLE_ENTER as u8 {
                        // Enter
                        // カーソルをスペースで消す
                        console.put_chr(b' ', false);
                        let cmd_end = console.cursor_x as usize / 8 - 2;
                        console.cmdline[cmd_end] = 0;
                        console.cons_newline();

                        console.run_cmd(fat, memtotal);
                        // プロンプト表示
                        console.put_chr(b'>', true);
                        // cursor_x = 16;
                    } else {
                        // 一般文字
                        if console.cursor_x < MAX_CURSOR_X {
                            console.cmdline[console.cursor_x as usize / 8 - 2] = key;
                            console.put_chr(key, true)
                        }
                    }
                }
            }

            if console.cursor_on {
                boxfill(
                    sheet.buf_addr,
                    sheet.width as isize,
                    console.cursor_c,
                    console.cursor_x,
                    console.cursor_y,
                    console.cursor_x + 7,
                    console.cursor_y + 15,
                );
                sheet_manager.refresh(
                    console.sheet_index, 
                    console.cursor_x as i32, 
                    console.cursor_y as i32, 
                    console.cursor_x as i32 + 8, 
                    console.cursor_y as i32 + 16
                );
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn bin_api(
    _edi: i32,
    _esi: i32,
    _ebp: i32,
    _esp: i32,
    ebx: i32,
    edx: i32,
    ecx: i32,
    eax: i32,
) {
    let cs_base = unsafe { *(CS_BASE_ADDR as *const usize) };
    let console_addr = unsafe { *(CONSOLE_ADDR as *const usize) };
    let console = unsafe { &mut *(console_addr as *mut Console) };
    if edx == 1 {
        // 1文字出力
        console.put_chr(eax as u8, true);
    } else if edx == 2 {
        // 0がくるまで1文字ずつ出力
        let mut i = 0;
        loop {
            let chr = unsafe { *((ebx as usize + i as usize + cs_base) as *const u8) };
            if chr == 0 {
                break;
            }
            console.put_chr(chr, true);
            i += 1;
        }
    } else if edx == 3 {
        // 指定した文字数出力
        for i in 0..ecx {
            let chr = unsafe { *((ebx as usize + i as usize + cs_base) as *const u8) };
            console.put_chr(chr, true);
        }
    }
}


#[repr(C, packed)]
pub struct Console {
    pub cursor_x: isize,
    pub cursor_y: isize,
    pub cursor_c: Color,
    pub cursor_on: bool,
    pub sheet_index: usize,
    pub sheet_manager_addr: usize,
    pub cmdline: [u8; 30],
}

impl Console {
    pub fn new(sheet_index: usize, sheet_manager_addr: usize) -> Console {
        Console {
            cursor_x: MIN_CURSOR_X - 8,
            cursor_y: MIN_CURSOR_Y,
            cursor_c: Color::Black,
            cursor_on: false,
            sheet_index,
            sheet_manager_addr,
            cmdline: [0; 30],
        }
    }

    pub extern "C" fn put_chr(&mut self, chr: u8, move_cursor: bool) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
        write_with_bg!(
            sheet_manager,
            self.sheet_index,
            sheet.width,
            sheet.height,
            self.cursor_x,
            self.cursor_y,
            Color::White,
            Color::Black,
            1,
            "{}",
            chr as char,
        );
        if move_cursor {
            self.cursor_x += 8;
        }
    }

    pub fn cons_newline(&mut self) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
    
        if self.cursor_y < MAX_CURSOR_Y {
            self.cursor_y += 16; // 次の行へ
        } else {
            // スクロール
            for y in MIN_CURSOR_Y..MAX_CURSOR_Y {
                for x in 8..(MAX_CURSOR_X + 8) {
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
            for y in MAX_CURSOR_Y..(MAX_CURSOR_Y + 16) {
                for x in 8..(MAX_CURSOR_X + 8) {
                    let x = x as usize;
                    let y = y as usize;
                    // スクロール
                    let ptr = unsafe {
                        &mut *((sheet.buf_addr + x + y * sheet.width as usize) as *mut u8)
                    };
                    *ptr = Color::Black as u8;
                }
            }
            sheet_manager.refresh(self.sheet_index, 8, MIN_CURSOR_Y as i32, MAX_CURSOR_X as i32 + 8, MAX_CURSOR_Y as i32 + 16);
        }
    }

    pub fn run_cmd(&mut self, fat: &[u32; MAX_FAT], memtotal: u32) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
        self.cursor_x = 8;
        let cmdline = self.cmdline.clone();
        let cmdline_strs = cmdline.split(|s| *s == 0 || *s == b' ');
        let mut cmdline_strs = cmdline_strs.skip_while(|cmd| cmd.len() == 0);
        let cmd = cmdline_strs.next();
        if cmd.is_none() {
            self.display_error("Bad command.");
        }
        let cmd = cmd.unwrap();
        let cmd_str = from_utf8(&cmd).unwrap();

        // コマンド実行
        match cmd_str {
            "mem" => self.cmd_mem(memtotal),
            "clear" => self.cmd_clear(),
            "ls" => self.cmd_ls(),
            "cat" => self.cmd_cat(cmdline_strs, fat),
            "hlt" => self.cmd_hlt(fat),
            _ => self.cmd_app(&cmd, fat),
        }
        
    }

    pub fn cmd_mem(&mut self, memtotal: u32) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
        let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
        // memコマンド
        write_with_bg!(
            sheet_manager,
            self.sheet_index,
            sheet.width,
            sheet.height,
            8,
            self.cursor_y,
            Color::White,
            Color::Black,
            30,
            "total   {}MB",
            memtotal / (1024 * 1024)
        );
        self.cons_newline();
        write_with_bg!(
            sheet_manager,
            self.sheet_index,
            sheet.width,
            sheet.height,
            8,
            self.cursor_y,
            Color::White,
            Color::Black,
            30,
            "free {}KB",
            memman.total() / 1024
        );
        self.cons_newline();
        self.cons_newline();
    }

    pub fn cmd_clear(&mut self) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
        for y in MIN_CURSOR_Y..(MIN_CURSOR_Y + 16) {
            for x in (MIN_CURSOR_X - 8)..(8 + MAX_CURSOR_X) {
                let x = x as usize;
                let y = y as usize;
                let ptr = unsafe {
                    &mut *((sheet.buf_addr + x + y * sheet.width as usize) as *mut u8)
                };
                *ptr = Color::Black as u8;
            }
        }    
        sheet_manager.refresh(
            self.sheet_index, 8, 28, 8+240, 28+128
        );
        self.cursor_y = MIN_CURSOR_Y;
    }

    pub fn cmd_ls(&mut self) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
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
                        self.sheet_index,
                        sheet.width,
                        sheet.height,
                        8,
                        self.cursor_y,
                        Color::White,
                        Color::Black,
                        30,
                        "{:>8}.{:>3}     {:>7}",
                        from_utf8(&finfo.name).unwrap(),
                        from_utf8(&finfo.ext).unwrap(),
                        finfo.size
                    );
                    self.cons_newline();
                }
            }
        }
        self.cons_newline();
    }

    pub fn cmd_cat<'a>(
        &mut self,
        mut cmdline_strs: impl Iterator<Item = &'a [u8]>,
        fat: &[u32; MAX_FAT],
    ) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
        let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
        let mut filename = cmdline_strs.next();
        if filename.is_none() {
            self.display_error("File not found");
        } else {
            let filename = filename.unwrap();
            let target_finfo = search_file(filename);
            if let Some(finfo) = target_finfo {
                // ファイルが見つかった場合
                let content_addr = memman.alloc_4k(finfo.size).unwrap() as usize;
                finfo.file_loadfile(content_addr, fat, ADR_DISKIMG + 0x003e00);
                self.cursor_x = 8;
                for c in 0..finfo.size {
                    let p = unsafe { *((content_addr + c as usize) as *const u8) };
                    if p == 0x09 {
                        // タブ
                        loop {
                            self.put_chr(b' ', true);
                            if self.cursor_x == 8 + MAX_CURSOR_X {
                                self.cursor_x = 8;
                                self.cons_newline();
                            }
                            if ((self.cursor_x - 8) & 0x1f) == 0 {
                                // 32で割り切れたら
                                break;
                            }
                        }
                    } else if p == 0x0a {
                        // 改行
                        self.cursor_x = 8;
                        self.cons_newline();
                    } else if p == 0x0d {
                        // 復帰（とりあえず何もしない）
                    } else {
                        // 普通の文字
                        self.put_chr(p, true);
                        if self.cursor_x == 8 + MAX_CURSOR_X {
                            // 右端まで来たので改行
                            self.cursor_x = 8;
                            self.cons_newline();
                        }
                    }
                }
                self.cons_newline();
                memman.free_4k(content_addr as u32, finfo.size).unwrap();
            } else {
                self.display_error("File not found");
            }
        }
    }

    pub fn cmd_hlt(&mut self, fat: &[u32; MAX_FAT]) {
        let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
        let mut target_finfo = search_file(b"hlt.bin");
        if let Some(finfo) = target_finfo {
            let content_addr = memman.alloc_4k(finfo.size).unwrap() as usize;
            finfo.file_loadfile(content_addr, fat, ADR_DISKIMG + 0x003e00);
            let gdt = unsafe { &mut *((ADR_GDT + 1003 * 8) as *mut SegmentDescriptor) };    // 1,2,3はdescriptor_table.rsで，1002まではmt.rsで使用済み
            *gdt = SegmentDescriptor::new(finfo.size - 1, content_addr as i32, AR_CODE32_ER);
            farcall(0, 1003 * 8);
            memman.free_4k(content_addr as u32, finfo.size).unwrap();
            self.cons_newline();
        } else {
            self.display_error("File not found");
        }
    }

    pub fn cmd_app<'a>(&mut self, filename: &'a [u8], fat: &[u32; MAX_FAT]) {
        let memman = unsafe { &mut *(MEMMAN_ADDR as *mut MemMan) };
        let mut finfo = search_file(filename);
        if let Some(finfo) = finfo {
            let content_addr = memman.alloc_4k(finfo.size).unwrap() as usize;
            finfo.file_loadfile(content_addr, fat, ADR_DISKIMG + 0x003e00);
            let gdt = unsafe { &mut *((ADR_GDT + 1003 * 8) as *mut SegmentDescriptor) };    // 1,2,3はdescriptor_table.rsで，1002まではmt.rsで使用済み
            *gdt = SegmentDescriptor::new(finfo.size - 1, content_addr as i32, AR_CODE32_ER);
            farcall(0, 1003 * 8);
            memman.free_4k(content_addr as u32, finfo.size).unwrap();
            self.cons_newline();
        } else {
            self.display_error("File not found");
        }
    }

    pub fn display_error(&mut self, error_massage: &'static str) {
        let sheet_manager = unsafe { &mut *(self.sheet_manager_addr as *mut SheetManager) };
        let sheet = sheet_manager.sheets_data[self.sheet_index];
        write_with_bg!(
            sheet_manager,
            self.sheet_index,
            sheet.width,
            sheet.height,
            8,
            self.cursor_y,
            Color::White,
            Color::Black,
            30,
            "{}",
            error_massage
        );
        self.cons_newline();
        self.cons_newline();
    }
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

#[no_mangle]
pub extern "C" fn console_put_char(console: &mut Console, char_num: u8, move_cursor: bool) {
    console.put_chr(char_num, move_cursor);
}

