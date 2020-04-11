use crate::asm;
use crate::handler;

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct SegmentDescriptor {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access_rigth: u8,
    limit_high: u8,
    base_high: u8,
}

impl SegmentDescriptor {
    fn new(limit: mut u32, base: i32, ar: mut i32) {
        if limit > 0xfffff {
            ar |= 0x8000;   // G_bit = 1
            limit /=0x1000;
        }
        SegmentDescriptor {
            limit_low: limit as u16,
            base_low: base as u16,
            base_mid: (base >> 16) as u8,
            access_rigth: ar as u8,
            limit_high: ((limit >> 16) as u8 & 0x0f) | ((ar >> 8) as u8 & 0xf0);
            base_high: (base >> 24) as u8 & 0xff;
        }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct GateDescriptor {
    offset_low: u16,
    selector: u16,
    dw_count: u8,
    access_rigth: u8,
    offset_high: u16,
}

impl GateDescriptor {
    fn new(offset: i32, selector: i32, ar: i32) {
        GateDescriptor {
            offset_low: offset as u16,
            selector: selector as u16,
            dw_count: (ar >> 8) as u8,
            access_rigth: ar as u8,
            offset_high: (offset >> 16) as u16,
        }
    }
}

const ADR_GDT: i32 = 0x00270000;
const LIMIT_GDT: i32 = 0x0000ffff;
const ADR_IDT: i32 = 0x0026f800;
const LIMIT_IDT: i32 = 0x000007ff;
const ADR_BOTPAK: i32 = 0x00280000;
const LIMIT_BOTPAK: u32 = 0x0007ffff;
const AR_INTGATE32: i32 = 0x008e;
const AR_DATA32_RW: i32 = 0x4092;
const AR_CODE32_ER: i32 = 0x409a;

pub fn init() {
    // GDTの初期化
    for i in 0..8192 {
        let gdt = unsafe { &mut *((ADR_GDT + i * 8) as *mut SegmentDescriptor) }
        *gdt = SegmentDescriptor::new(0, 0, 0);
    }
    let gdt = unsafe { &mut *((ADR_GDT + 1 * 8) as *mut SegmentDescriptor) };
    *gdt = SegmentDescriptor::new(0xffffffff, 0x00000000, AR_DATA32_RW);
    let gdt = unsafe { &mut *((ADR_GDT + 2 * 8) as *mut SegmentDescriptor) };
    *gdt = SegmentDescriptor::new(LIMIT_BOTPAK, ADR_BOTPAK, AR_CODE32_ER);
    load_gdtr(LIMIT_GDT, ADR_GDT);

    // IDTの初期化
    for i in 0..256 {
        let idt = unsafe { &mut *((ADR_IDT + i * 8) as *mut GateDescriptor) }
        *idt = GateDescriptor::new(0, 0, 0);
    }
    load_idtr(LIMIT_IDT, ADR_IDT);
}

