use lazy_static::lazy_static;
use spin::Mutex;

use crate::asm;
use crate::int::PIC0_OCW2;
use crate::fifo::FIFO8;

const PIT_CTRL: u32 = 0x0043;
const PIT_CNT0: u32 = 0x0040;

const MAX_TIMER: usize = 500;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum TIMERFLAG {
    AVAILABLE,
    ALLOC,
    USING,
}

#[derive(Copy, Clone)]
pub struct TIMER {
    pub timeout: u32,
    pub flags: TIMERFLAG,
    pub fifo: usize,
    pub data: u8,
}

impl TIMER {
    pub fn new() -> TIMER {
        TIMER {
            timeout: 0,
            flags: TIMERFLAG::AVAILABLE,
            fifo: 0,
            data: 0,
        }
    }
}

pub struct TIMERCTL {
    pub count: u32,
    pub timer: [TIMER; MAX_TIMER],
}

impl TIMERCTL {
    pub fn new() -> TIMERCTL {
        TIMERCTL {
            count: 0,
            timer: [TIMER::new(); MAX_TIMER],
        }
    }

    pub fn alloc(&mut self) -> Result<usize, &'static str> {
        for i in 0..MAX_TIMER {
            if self.timer[i].flags == TIMERFLAG::AVAILABLE {
                self.timer[i].flags = TIMERFLAG::ALLOC;
                return Ok(i);
            }
        }
        return Err("CANNOT ASSUGN TIMER")
    }

    pub fn free(&mut self, i: usize) {
        self.timer[i].flags = TIMERFLAG::AVAILABLE;
    }

    pub fn init(&mut self, index: usize, fifo: &FIFO8, data: u8) {
        self.timer[index].fifo = fifo as *const FIFO8 as usize;
        self.timer[index].data = data;
    }

    pub fn settime(&mut self, index: usize, timeout: u32) {
        self.timer[index].timeout = timeout + self.count;
        self.timer[index].flags = TIMERFLAG::USING;
    }
}

lazy_static! {
    pub static ref timerctl: Mutex<TIMERCTL> = Mutex::new(TIMERCTL::new());
}

pub fn init_pit() {
    asm::out8(PIT_CTRL, 0x34);
    asm::out8(PIT_CNT0, 0x9c);
    asm::out8(PIT_CNT0, 0x2e);
}

pub extern "C" fn inthandler20() {
    asm::out8(PIC0_OCW2, 0x60); // IRQ-00受付完了をPICに通知
    let mut tc = timerctl.lock();
    tc.count += 1; // カウントアップ
    for i in 0..MAX_TIMER {
        if tc.timer[i].flags == TIMERFLAG::USING {
            if tc.timer[i].timeout <= tc.count {
                let fifo = unsafe{ &*(tc.timer[i].fifo as *const FIFO8) };
                fifo.put(tc.timer[i].data).unwrap();
                tc.timer[i].flags = TIMERFLAG::ALLOC;
            }
        }
    }
}