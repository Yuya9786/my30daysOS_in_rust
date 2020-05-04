use lazy_static::lazy_static;
use spin::Mutex;

use crate::asm;
use crate::interrupt::PIC0_OCW2;
use crate::fifo::Fifo;

const PIT_CTRL: u32 = 0x0043;
const PIT_CNT0: u32 = 0x0040;

const MAX_TIMER: usize = 500;

#[derive(Copy, Clone, Eq, PartialEq)]
pub enum TimerFlag {
    AVAILABLE,
    ALLOC,
    USING,
}

#[derive(Copy, Clone)]
pub struct Timer {
    pub timeout: u32,
    pub flags: TimerFlag,
    pub fifo: usize,
    pub data: u8,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            timeout: 0,
            flags: TimerFlag::AVAILABLE,
            fifo: 0,
            data: 0,
        }
    }
}

pub struct TimerManager {
    pub count: u32,
    pub next: u32,
    pub using: u32,
    pub timers: [usize; MAX_TIMER],
    pub timers_data: [Timer; MAX_TIMER],
}

impl TimerManager {
    pub fn new() -> TimerManager {
        TimerManager {
            count: 0,
            next: 0xffffffff,
            using: 0,
            timers: [0; MAX_TIMER],
            timers_data: [Timer::new(); MAX_TIMER],
        }
    }

    pub fn alloc(&mut self) -> Result<usize, &'static str> {
        for i in 0..MAX_TIMER {
            if self.timers_data[i].flags == TimerFlag::AVAILABLE {
                self.timers_data[i].flags = TimerFlag::ALLOC;
                return Ok(i);
            }
        }
        return Err("CANNOT ASSUGN TIMER")
    }

    pub fn free(&mut self, i: usize) {
        let mut timer = &mut self.timers_data[i];
        timer.flags = TimerFlag::AVAILABLE;
    }

    pub fn init_timer(&mut self, index: usize, fifo: &Fifo, data: u8) {
        let mut timer = &mut self.timers_data[index];
        timer.fifo = fifo as *const Fifo as usize;
        timer.data = data;
    }

    pub fn set_time(&mut self, index: usize, timeout: u32) {
        let mut timer = &mut self.timers_data[index];
        timer.timeout = timeout + self.count;
        timer.flags = TimerFlag::USING;
        let eflags = asm::load_eflags();
        asm::cli();
        // どこに入れれば良いか探す
        let mut insert_index: usize = 0;
        for i in 0..self.using {
            insert_index = i as usize;
            let t = self.timers_data[self.timers[i as usize]];
            if t.timeout >= timeout + self.count {
                break;
            }
        }
        self.using += 1;
        // 後ろをずらす
        let mut j = self.using as usize;
        while j > insert_index {
            self.timers[j] = self.timers[j-1];
            j -= 1;
        }
        // 開いた隙間に入れる
        self.timers[insert_index] = index;
        self.next = self.timers_data[self.timers[0]].timeout;
        asm::store_eflags(eflags);
    }
}

lazy_static! {
    pub static ref TIMER_MANAGER: Mutex<TimerManager> = Mutex::new(TimerManager::new());
}

pub fn init_pit() {
    asm::out8(PIT_CTRL, 0x34);
    asm::out8(PIT_CNT0, 0x9c);
    asm::out8(PIT_CNT0, 0x2e);
}

pub extern "C" fn inthandler20() {
    asm::out8(PIC0_OCW2, 0x60); // IRQ-00受付完了をPICに通知
    let mut tm = TIMER_MANAGER.lock();
    tm.count += 1; // カウントアップ
    if tm.next > tm.count {
        return;
    }
    
    let mut timeout_count = 0;
    for i in 0..tm.using {
        // timersのデータは全て動作中なので，flagsは確認しない
        timeout_count = i;
        let timer_index = tm.timers[i as usize];
        let t = tm.timers_data[timer_index];
        if t.timeout > tm.count {
            break;
        }
        {   // タイムアウト
            let mut t_mut = &mut tm.timers_data[timer_index];
            t_mut.flags = TimerFlag::ALLOC;
        }
        let fifo = unsafe { &*(t.fifo as *const Fifo) };
        fifo.put(t.data).unwrap();
    }
    // ちょうどi個のタイマがタイムアウトした，残りをずらす
    tm.using -= timeout_count;
    for i in 0..tm.using {
        tm.timers[i as usize] = tm.timers[(timeout_count + i) as usize];
    }
    if tm.using > 0 {
        tm.next = tm.timers_data[tm.timers[0]].timeout;
    } else {
        tm.next = 0xffffffff;
    }
}