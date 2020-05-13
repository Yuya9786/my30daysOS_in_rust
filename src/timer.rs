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
    pub next: Option<usize>,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            timeout: 0,
            flags: TimerFlag::AVAILABLE,
            fifo: 0,
            data: 0,
            next: None,
        }
    }
}

pub struct TimerManager {
    pub count: u32,
    pub next_time: u32,
    pub t0: Option<usize>,
    pub timers_data: [Timer; MAX_TIMER],
}

impl TimerManager {
    pub fn new() -> TimerManager {
        let mut tm = TimerManager {
            count: 0,
            next_time: 0xffffffff,
            t0: Some(MAX_TIMER - 1),
            timers_data: [Timer::new(); MAX_TIMER],
        };
        // 番兵（お留守番君）
        tm.timers_data[MAX_TIMER-1] = Timer {
            timeout: 0xffffffff,
            flags: TimerFlag::USING,
            fifo: 0,
            data: 0,
            next: None,
        };
        tm
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

    pub fn init_timer(&mut self, index: usize, fifo: usize, data: u8) {
        let mut timer = &mut self.timers_data[index];
        timer.fifo = fifo;
        timer.data = data;
    }

    pub fn set_time(&mut self, index: usize, timeout: u32) {
        {
            let mut timer = &mut self.timers_data[index];
            timer.timeout = timeout + self.count;
            timer.flags = TimerFlag::USING;
        }
        if self.t0.is_none() {
            return;
        }
        let eflags = asm::load_eflags();
        asm::cli();
        let mut t_index = self.t0.unwrap();
        if &self.timers_data[index].timeout <= &self.timers_data[t_index].timeout {
            // 先頭に入れる
            self.t0 = Some(index);
            let mut timer = &mut self.timers_data[index];
            timer.next = Some(t_index);
            self.next_time = timer.timeout;
            asm::store_eflags(eflags);
            return;
        }
        // どこに入れれば良いか探す
        let mut s_index: usize; 
        loop {
            s_index = t_index;
            if self.timers_data[t_index].next.is_none() {
                asm::store_eflags(eflags);
                break;
            }
            t_index = self.timers_data[t_index].next.unwrap();
            if &self.timers_data[index].timeout <= &self.timers_data[t_index].timeout {
                {
                    let mut s = &mut self.timers_data[s_index];
                    s.next = Some(index);
                }
                {
                    let mut timer = &mut self.timers_data[index];
                    timer.next = Some(t_index);
                }
                asm::store_eflags(eflags);
                return;
            }
        }
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

pub static mut NEED_SWITCH: bool = false;

pub extern "C" fn inthandler20() {
    asm::out8(PIC0_OCW2, 0x60); // IRQ-00受付完了をPICに通知
    let mut tm = TIMER_MANAGER.lock();
    tm.count += 1; // カウントアップ
    if tm.next_time > tm.count {
        return;
    }
    
    let mut timer_index = tm.t0;
    let mut need_taskswitch = false;
    loop {
        let t_index = timer_index.unwrap();
        // timersのデータは全て動作中なので，flagsは確認しない
        if tm.timers_data[t_index].timeout > tm.count {
            break;
        }
        // タイムアウト
        let mut t = &mut tm.timers_data[t_index];
        t.flags = TimerFlag::ALLOC;
        if t_index != unsafe { crate::multi_task::MT_TIMER_INDEX } {
            let fifo = unsafe { &mut *(t.fifo as *mut Fifo) };
            fifo.put(t.data as u32).unwrap();
        } else {
            need_taskswitch = true;
        }
        timer_index = t.next;
    }
    tm.t0 = timer_index;
    if let Some(t_index) = timer_index {
        tm.next_time = tm.timers_data[t_index].timeout;
    }
    if need_taskswitch {
        unsafe {
            NEED_SWITCH = true;
        }
    }
}