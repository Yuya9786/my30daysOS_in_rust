use crate::descriptor_table::{ADR_GDT, AR_TSS32, SegmentDescriptor};
use crate::asm;

const MAX_TASKS: usize = 100;   // 最大タスク数
const TASK_GDT0: usize = 3;       // TSSをGDTの何番から割り当てるのか

#[derive(Debug, Default, Copy, Clone)]
#[repr(C, packed)]
pub struct TSS {
    pub backlink: i32,
    pub esp0: i32,
    pub ss0: i32,
    pub esp1: i32,
    pub ss1: i32,
    pub esp2: i32,
    pub ss2: i32,
    pub cr3: i32,
    pub eip: i32,
    pub eflags: i32,
    pub eax: i32,
    pub ecx: i32,
    pub edx: i32,
    pub ebx: i32,
    pub esp: i32,
    pub ebp: i32,
    pub esi: i32,
    pub edi: i32,
    pub es: i32,
    pub cs: i32,
    pub ss: i32,
    pub ds: i32,
    pub fs: i32,
    pub gs: i32,
    pub ldtr: i32,
    pub iomap: i32,
}

#[derive(Debug, Clone, Copy)]
pub struct Task {
    pub sel: i32,
    pub flags: TaskFlag,
    pub tss: TSS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFlag {
    AVAILABLE,
    ALLOC,
    RUNNING,
}

impl Task {
    fn new() -> Task {
        Task {
            sel: 3 * 8,
            flags: TaskFlag::AVAILABLE,
            tss: Default::default(),
        }
    }
}

pub static mut TASK_MANAGER_ADDR: usize = 0;

pub struct TaskManager {
    pub running: usize,   // 動作しているタスクの数
    pub now: usize,       // 現在動作しているタスクがどれだか分かるようにするための変数
    pub tasks: [usize; MAX_TASKS],
    pub tasks_data: [Task; MAX_TASKS],
}

impl TaskManager {
    pub fn new() -> TaskManager {
        TaskManager {
            running: 0,
            now: 0,
            tasks: [0; MAX_TASKS],
            tasks_data: [Task::new(); MAX_TASKS],
        }
    }

    pub fn init(&mut self) {
        for i in 0..MAX_TASKS {
            let mut task = &mut self.tasks_data[i];
            task.sel = (TASK_GDT0 + i) as i32 * 8;
            let gdt = unsafe { &mut *((ADR_GDT + (TASK_GDT0 + i) as i32 * 8) as *mut SegmentDescriptor) };
            *gdt = SegmentDescriptor::new(103, &(task.tss) as *const TSS as i32, AR_TSS32)
        }
        let task_index = self.alloc().unwrap();
        let mut task = &mut self.tasks_data[task_index];
        task.flags = TaskFlag::RUNNING;     // 動作中
        self.running = 1;
        self.now = 0;
        self.tasks[0] = task_index;
        asm::load_tr(task.sel);
        let task_timer = TIMER_MANAGER.lock().alloc().unwrap();
        TIMER_MANAGER.lock().set_time(task_timer, 2);
        unsafe {
            MT_TIMER_INDEX = task_timer;
        }
    }

    pub fn alloc(&mut self) -> Result<usize, &'static str> {
        for i in 0..MAX_TASKS {
            if self.tasks_data[i].flags == TaskFlag::AVAILABLE {
                let mut task = &mut self.tasks_data[i];
                task.flags = TaskFlag::ALLOC;    // 使用中
                task.tss.eflags = 0x00000202;       // IF = 1
                task.tss.eax = 0;       // とりあえず0
                task.tss.ecx = 0;
                task.tss.edx = 0;
                task.tss.ebx = 0;
                task.tss.ebp = 0;
                task.tss.esi = 0;
                task.tss.edi = 0;
                task.tss.es = 0;
                task.tss.ds = 0;
                task.tss.fs = 0;
                task.tss.gs = 0;
                task.tss.ldtr = 0;
                task.tss.iomap = 0x40000000;
                return Ok(i);
            }
        }
        return Err("CANNOT ALLOC TASK");    // 全部使用中
    }

    pub fn run(&mut self, task_index: usize) {
        let mut task = &mut self.tasks_data[task_index];
        task.flags = TaskFlag::RUNNING;
        self.tasks[self.running] = task_index;
        self.running += 1;
    }

    pub fn switch(&mut self) {
        TIMER_MANAGER.lock().set_time(unsafe { MT_TIMER_INDEX }, 2);
        if self.running >= 2 {
            self.now += 1;
            if self.now == self.running {   // 先頭のタスクに戻す
                self.now = 0;
            }
            crate::asm::farjmp(0, self.tasks_data[self.tasks[self.now]].sel);
        }
    }
}

use crate::timer::TIMER_MANAGER;

pub static mut MT_TIMER_INDEX: usize = 0;
pub static mut MT_TR: i32 = 3 * 8;

pub fn mt_init() {
    let timer_index_ts = TIMER_MANAGER.lock().alloc().unwrap();
    TIMER_MANAGER.lock().set_time(timer_index_ts, 2);
    unsafe {
        MT_TIMER_INDEX = timer_index_ts;
    }
}

pub fn mt_taskswitch() {
    if unsafe { MT_TR } == 3 * 8 {
        unsafe {
            MT_TR = 4 * 8;
        }
        TIMER_MANAGER.lock().set_time(unsafe { MT_TIMER_INDEX }, 2);
        crate::asm::farjmp(0, 4 * 8);
    } else {
        unsafe {
            MT_TR = 3 * 8;
        }
        TIMER_MANAGER.lock().set_time(unsafe { MT_TIMER_INDEX }, 2);
        crate::asm::farjmp(0, 3 * 8);
    }
}

