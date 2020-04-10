#![no_std]
#![feature(asm)]
#![feature(start)]

use core::panic::PanicInfo;

mod vga;
mod asm;

#[no_mangle]
fn hlt() {
    unsafe {
        asm!("hlt");
    }
}

#[no_mangle]
fn write_mem8(addr: u32, data: u8) {
    let ptr = unsafe { &mut *(addr as *mut u8) };
    *ptr = data;
}

#[no_mangle]
#[start]
pub extern "C" fn HariMain() -> ! {
    vga::set_palette();
    // for i in 0xa000..0xaffff {
    //     write_mem8(i, (i & 0x0f) as u8);
    // }
    use vga::Color::*;
    let vram = unsafe { &mut *(0xa0000 as *mut u8) };
    let xsize = 320;    // 画面サイズ (320 * 200)
    let ysize = 200;

    vga::boxfill8(vram, xsize, DarkCyan, 0, 0, xsize - 1, ysize - 29);
	vga::boxfill8(vram, xsize, LightGray, 0, ysize - 28, xsize - 1, ysize - 28);
	vga::boxfill8(vram, xsize, White, 0, ysize - 27, xsize - 1, ysize - 27);
	vga::boxfill8(vram, xsize, LightGray, 0, ysize - 26, xsize - 1, ysize - 1);

	vga::boxfill8(vram, xsize, White, 3, ysize - 24, 59, ysize - 24);
	vga::boxfill8(vram, xsize, White, 2, ysize - 24, 2, ysize - 4);
	vga::boxfill8(vram, xsize, DarkYellow, 3, ysize - 4, 59, ysize - 4);
	vga::boxfill8(vram, xsize, DarkYellow, 59, ysize - 23, 59, ysize - 5);
	vga::boxfill8(vram, xsize, Black,  2, ysize -  3, 59, ysize - 3);
	vga::boxfill8(vram, xsize, Black, 60, ysize - 24, 60, ysize - 3);

	vga::boxfill8(vram, xsize, DarkGray, xsize - 47, ysize - 24, xsize - 4, ysize - 24);
	vga::boxfill8(vram, xsize, DarkGray, xsize - 47, ysize - 23, xsize - 47, ysize - 4);
	vga::boxfill8(vram, xsize, White, xsize - 47, ysize - 3, xsize - 4, ysize - 3);
	vga::boxfill8(vram, xsize, White, xsize - 3, ysize - 24, xsize - 3, ysize - 3);
    loop {
        hlt()
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    // println!("{}", info);
    loop {
        hlt()
    }
}