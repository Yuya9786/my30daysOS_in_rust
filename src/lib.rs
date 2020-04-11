#![no_std]
#![feature(asm)]
#![feature(start)]

use core::panic::PanicInfo;

mod vga;
mod asm;
mod fonts;
mod dsctbl;
mod int;

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
pub extern "C" fn HariMain() -> ! {
    use vga::{Color, Screen, ScreenWriter};
    use asm::{hlt, sti};

    dsctbl::init();
    int::init_pic();
    sti();
    

    let mut screen = Screen::new();
    let binfo = BOOTINFO::new();
    screen.init();
    
    let mut writer = ScreenWriter::new(screen, Color::White, 10, 10);
    use core::fmt::Write;
    write!(writer, "ABC\nabc\n").unwrap();
    write!(writer, "10 * 3 = {}\n", 10 * 3).unwrap();
    write!(
        writer,
        "Long string Long string Long string Long string Long string Long string Long string\n"
    )
    .unwrap();

    int::allow_input();

    loop {
        hlt();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // println!("{}", info);
    loop {
        asm::hlt()
    }
}