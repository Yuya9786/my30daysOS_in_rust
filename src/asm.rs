pub fn load_eflags() -> i32 {
    let result: i32;
    unsafe {
        asm!("PUSHFD" : : : : "intel");
        asm!("POP EAX" : "={EAX}"(result) : : : "intel");
    }
    result
}

pub fn store_eflags(flags: i32) {
    unsafe {
        asm!("PUSH EAX" : : "EAX"(flags) : : "intel");
        asm!("POPFD");
    }
}

pub fn cli() {
    unsafe {
        asm!("CLI" : : : : "intel");
    }
}

pub fn out8(port: u32, data: u8) {
    unsafe {
        asm!("OUT DX,AL" : : "{EDX}"(port), "{AL}"(data) : : "intel");
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C, packed)]
struct Dtr {
    limit: i16,
    base: i32,
}

pub fn load_gdtr(limit: i32, adr: i32) {
    unsafe {
        asm!("LGDT ($0)" :: "r"(&Dtr { limit: limit as i16, base: adr } ) : "memory");
    }
}

pub fn load_idtr(limit: i32, adr: i32) {
    unsafe {
        asm!("LIDT ($0)" :: "r"(&Dtr { limit: limit as i16, base: adr }) : "memory");
    }
}