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

    dsctbl::init();
    int::init_pic();
    sti();
    int::allow_input();
    let mut screen = Screen::new();
    screen.init();
    int::enable_mouse();
    loop {
        cli();
        if keyfifo.lock().status() + mousefifo.lock().status() == 0 {
            stihlt();
        } else {
            if keyfifo.lock().status() != 0 {
                let i = keyfifo.lock().get().unwrap();
                sti();
                (Screen::new()).boxfill8(Color::DarkCyan, 0, 0, 16, 16);
                let mut writer = ScreenWriter::new(Screen::new(), Color::White, 0, 0);
                write!(writer, "{:02x}", i).unwrap();
            } else if mousefifo.lock().status() != 0 {
                let i = mousefifo.lock().get().unwrap();
                sti();
                (Screen::new()).boxfill8(Color::DarkCyan, 32, 0, 48, 16);
                let mut writer = ScreenWriter::new(Screen::new(), Color::White, 32, 0);
                write!(writer, "{:02x}", i).unwrap();
            }
        }
    }
}


#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    use vga::{Screen, ScreenWriter};
    let mut screen = Screen::new();
    screen.init();
    let mut writer = ScreenWriter::new(screen, vga::Color::LightRed, 0, 0);
    use core::fmt::Write;
    write!(writer, "[ERR] {:?}", info).unwrap();
    loop {
        asm::hlt()
    }
}