use crate::descriptor_table::{ADR_GDT, AR_TSS32, SegmentDescriptor};
use crate::asm;

const MAX_TASKS: usize = 1000;
const TASK_GDT0: usize = 3;       // TSSをGDTの何番から割り当てるのか
const MAX_TASKS_LV: usize = 100;
const MAX_TASKLEVELS: usize = 10;

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
    pub level: usize,
    pub priority: u32,
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
            level: 0,
            priority: 2,
            tss: Default::default(),
        }
    }
}

#[derive(Clone, Copy)]
pub struct TaskLevel {
    pub running: usize,     // 動作しているタスクの数
    pub now: usize,         // 現在動作しているタスクがどれだか分かるようにする
    pub tasks: [usize; MAX_TASKS_LV],   // tasks_dataのインデックス
}

impl TaskLevel {
    pub fn new() -> TaskLevel {
        TaskLevel {
            running: 0,
            now: 0,
            tasks: [0; MAX_TASKS_LV],
        }
    }
}

pub static mut TASK_MANAGER_ADDR: usize = 0;

pub struct TaskManager {
    pub now_lv: usize,      // 現在動作中のレベル
    pub lv_change: bool,    // 次回タスクスイッチの時に，レベルを変えた方がいいかどうか      
    pub level: [TaskLevel; MAX_TASKLEVELS],
    pub tasks_data: [Task; MAX_TASKS],
}

impl TaskManager {
    pub fn new() -> TaskManager {
        TaskManager {
            now_lv: 0,
            lv_change: false,
            level: [TaskLevel::new(); MAX_TASKLEVELS],
            tasks_data: [Task::new(); MAX_TASKS],
        }
    }

    pub fn init(&mut self) -> usize {
        // セグメント設定
        for i in 0..MAX_TASKS {
            let mut task = &mut self.tasks_data[i];
            task.sel = (TASK_GDT0 + i) as i32 * 8;
            let gdt = unsafe { &mut *((ADR_GDT + (TASK_GDT0 + i) as i32 * 8) as *mut SegmentDescriptor) };
            *gdt = SegmentDescriptor::new(103, &(task.tss) as *const TSS as i32, AR_TSS32)
        }

        // メインタスク設定
        let task_index = self.alloc().unwrap();
        {
            let mut task = &mut self.tasks_data[task_index];
            task.flags = TaskFlag::RUNNING;     // 動作中
            task.priority = 2;                  // 0.02秒
            task.level = 0;                     // 最高レベル
        }
        self.add(task_index);
        self.switchsub();                       // レベル設定
        let task = self.tasks_data[task_index];
        asm::load_tr(task.sel);
        let task_timer = TIMER_MANAGER.lock().alloc().unwrap();
        TIMER_MANAGER.lock().set_time(task_timer, task.priority);
        unsafe {
            MT_TIMER_INDEX = task_timer;
        }
        task_index
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

    pub fn run(&mut self, task_index: usize, level: i32, priority: u32) {
        let task = self.tasks_data[task_index];
        let level: usize = if level < 0 {
            task.level
        } else {
            level as usize
        };
        if priority > 0 {
            let mut task = &mut self.tasks_data[task_index];
            task.priority = priority;
        }

        if task.flags == TaskFlag::RUNNING && task.level != level {
            // 動作中のレベル変更
            self.remove(task_index);  // 実行するとflagsはALLOCになり下も実行される
        }
        if task.flags != TaskFlag::RUNNING {
            // スリープから起こされる
            let mut task = &mut self.tasks_data[task_index];
            task.level = level;
            self.add(task_index);
        }

        self.lv_change = true;
    }

    pub fn switch(&mut self) {
        let mut tl = &mut self.level[self.now_lv];
        let now_task_index = tl.tasks[tl.now];
        tl.now += 1;
        if tl.now == tl.running {
            tl.now = 0;
        }
        if self.lv_change {
            self.switchsub();
            tl = &mut self.level[self.now_lv];
        }
        let new_task_index = tl.tasks[tl.now];
        let new_task = &self.tasks_data[new_task_index];
        TIMER_MANAGER.lock().set_time(unsafe { MT_TIMER_INDEX }, new_task.priority);
        if new_task_index != now_task_index {
            crate::asm::farjmp(0, new_task.sel);
        }
    }

    pub fn sleep(&mut self, task_index: usize) {
        let task = &mut self.tasks_data[task_index];
        if task.flags == TaskFlag::RUNNING {
            let mut now_task = self.now();
            self.remove(task_index);
            if task_index == now_task {
                // 自分自身のスリープだったのでタスクスイッチが必要
                self.switchsub();
                now_task = self.now();  // 設定後の現在のタスク
                asm::farjmp(0, self.tasks_data[now_task].sel);
            }
        }
    }

    pub fn now(&self) -> usize {
        let tl = self.level[self.now_lv];
        tl.tasks[tl.now] 
    }

    pub fn add(&mut self, task_index: usize) {
        {
            let mut tl = &mut self.level[self.tasks_data[task_index].level];
            tl.tasks[tl.running] = task_index;
            tl.running += 1;
        }
        {
            let mut task = &mut self.tasks_data[task_index];
            task.flags = TaskFlag::RUNNING;
        }
    }

    pub fn remove(&mut self, task_index: usize) {
        let tl = &mut self.level[self.tasks_data[task_index].level];
        let mut index: usize = 0;
        // taskがどこにいるか調べる
        for i in 0..tl.running {
            index = i;
            if tl.tasks[i] == task_index {
                break;
            }
        }

        tl.running -= 1;
        if index < tl.now {
            tl.now -= 1;
        }
        if tl.now >= tl.running {
            tl.now = 0;
        }
        {
            let mut task = &mut self.tasks_data[task_index];
            task.flags = TaskFlag::ALLOC;
        }

        for i in index..tl.running {
            tl.tasks[i] = tl.tasks[i + 1];
        }
    }

    pub fn switchsub(&mut self) {
        let mut index: usize = 0;
        // 一番上のレベルを探す
        for i in 0..MAX_TASKLEVELS {
            index = i;
            if self.level[i].running > 0 {
                break;
            }
        }
        self.now_lv = index;
        self.lv_change = false;
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

