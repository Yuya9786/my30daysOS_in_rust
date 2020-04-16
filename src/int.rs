use crate::asm::{out8,in8};
use spin::Mutex;
use lazy_static::lazy_static;
use crate::fifo::FIFO8;

const PIC0_ICW1: u32 = 0x0020;
const PIC0_OCW2: u32 = 0x0020;
const PIC0_IMR: u32 = 0x0021;
const PIC0_ICW2: u32 = 0x0021;
const PIC0_ICW3: u32 = 0x0021;
const PIC0_ICW4: u32 = 0x0021;
const PIC1_ICW1: u32 = 0x00a0;
const PIC1_OCW2: u32 = 0x00a0;
const PIC1_IMR: u32 = 0x00a1;
const PIC1_ICW2: u32 = 0x00a1;
const PIC1_ICW3: u32 = 0x00a1;
const PIC1_ICW4: u32 = 0x00a1;

pub fn init_pic() {
    out8(PIC0_IMR, 0xff);   // 全ての割り込みを受け付けない
    out8(PIC1_IMR, 0xff);   // 全ての割り込みを受け付けない

    out8(PIC0_ICW1, 0x11);  // エッジトリガモード
    out8(PIC0_ICW2, 0x20);  // IRQ0-7は，INT20-27で受ける
    out8(PIC0_ICW3, 1 << 2);    // PIC1はIRQ2にて接続
    out8(PIC0_ICW4, 0x01);  // ノンバッファモード

    out8(PIC1_ICW1, 0x11); // エッジトリガモード
    out8(PIC1_ICW2, 0x28); // IRQ8-15は、INT28-2fで受ける
    out8(PIC1_ICW3, 2); // PIC1はIRQ2にて接続
    out8(PIC1_ICW4, 0x01); // ノンバッファモード

    out8(PIC0_IMR, 0xfb); // 11111011 PIC1以外は全て禁止
    out8(PIC1_IMR, 0xff); // 11111111 全ての割り込みを受け付けない
}

pub fn allow_input() {
    out8(PIC0_IMR, 0xf9); // PIC1とキーボードを許可(11111001)
    out8(PIC1_IMR, 0xef); // マウスを許可(11101111)
    init_keyboard();
}

const PORT_KEYDAT: u32 = 0x60;
const PORT_KEYSTA: u32 = 0x64;
const PORT_KEYCMD: u32 = 0x64;
const KEYSTA_SEND_NOTREADY: u8 = 0x02;
const KEYCMD_WRITE_MODE: u8 = 0x60;
const KBC_MODE: u8 = 0x47;
const KEYCMD_SENDTO_MOUSE: u8 = 0xd4;
const MOUSECMD_ENABLE: u8 = 0xf4;

pub fn wait_KBC_sendready() {
    // キーボードコントローラがデータ送信可能になるのを待つ
    loop {
        if in8(PORT_KEYSTA) & KEYSTA_SEND_NOTREADY == 0 {
            break;
        }
    }
    return;
}
pub fn init_keyboard() {
    // キーボードコントローラの初期化
    wait_KBC_sendready();
    out8(PORT_KEYCMD, KEYCMD_WRITE_MODE);
    wait_KBC_sendready();
    out8(PORT_KEYDAT, KBC_MODE);
}

pub fn enable_mouse() {
    // マウス有効
    wait_KBC_sendready();
    out8(PORT_KEYCMD, KEYCMD_SENDTO_MOUSE);
    wait_KBC_sendready();
    out8(PORT_KEYDAT, MOUSECMD_ENABLE);
}

lazy_static! {
    pub static ref keyfifo: Mutex<FIFO8> = Mutex::new(FIFO8::new(32));
    pub static ref mousefifo: Mutex<FIFO8> = Mutex::new(FIFO8::new(32));
}

pub extern "C" fn inthandler21() {
    out8(PIC0_OCW2, 0x61);  // IRQ-01受付完了をPIC0に通知
    let data = in8(PORT_KEYDAT);
    keyfifo.lock().put(data);
}

pub extern "C" fn inthandler2C() {
    out8(PIC1_OCW2, 0x64);  // IRQ-12受付完了をPIC1に通知
    out8(PIC0_OCW2, 0x62);  // IRQ-02 ..     PIC0
    let data = in8(PORT_KEYDAT);
    mousefifo.lock().put(data);
}

pub extern "C" fn inthandler27() {
    out8(PIC0_OCW2, 0x67);
}
