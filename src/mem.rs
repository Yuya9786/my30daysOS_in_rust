use crate::asm::*;

const EFLAGS_AC_BIT: u32 = 0x00040000;
const CR0_CACHE_DISABLE: u32 = 0x60000000;

const MEMMAN_FREES: u32 = 4090;  // 約32KB
pub const MEMMAN_ADDR: u32 = 0x003c0000;

#[derive(Debug, Clone, Copy, PartialEq)]
struct FREEINFO {   // 空き情報
    addr: u32,
    size: u32,
}

pub struct MEMMAN { // メモリ管理
    frees: u32,
    maxfrees: u32,
    lostsize: u32,
    losts: u32,
    free: [FREEINFO; MEMMAN_FREES as usize],
}

impl MEMMAN {
    pub fn new() -> MEMMAN {
        MEMMAN {
            frees: 0,       // 空き情報の個数
            maxfrees: 0,    // 状況観察用：freesの最大値
            lostsize: 0,    // 開放に失敗した合計サイズ
            losts: 0,       // 解放に失敗した回数
            free: [FREEINFO {
                addr: 0,
                size: 0,
            }; MEMMAN_FREES as usize],
        }
    }

    pub fn total(&self) -> u32 {    // 空きサイズの合計を報告
        let mut t = 0;
        for i in 0..self.frees {
            t += self.free[i as usize].size;
        }
        t
    }

    pub fn alloc(&mut self, size: u32) -> u32 {  // 確保
        for i in 0..self.frees as usize {
            let i = i as usize;
            if self.free[i].size >= size {  // 発見
                let a = self.free[i].addr;
                self.free[i].addr += size;
                self.free[i].size -= size;
                if self.free[i].size == 0 {
                    // free[i]がなくなったので前へ詰める
                    self.frees -= 1;
                    for j in i..self.frees as usize {
                        self.free[j] = self.free[j + 1];    // 構造体の代入
                    }
                }
                return a;
            }
        }
        0   // 空きがない
    }

    pub fn free(&mut self, addr: u32, size: u32) -> i32 {    // 解放
        // まとめやすさを考えると，free[]がaddr順に並んでいる方がいい
        // だからまず，どこに入れるべきかを決める
        let mut r: usize = 0;
        for i in 0..self.frees {
            let i = i as usize;
            if self.free[i].addr > addr {
                r = i;
                break;
            }
        }
        // free[i-1].addr < addr < free[i].addr
        if r > 0 {
            // 前がある
            if self.free[r-1].addr + self.free[r-1].size == addr {
                // 前の空き領域にまとめられる
                self.free[r-1].size += size;
                if r < self.frees as usize {
                    // 後ろもある
                    if addr + size == self.free[r].addr {
                        // なんと後ろともまとめられる
                        self.free[r-1].size += self.free[r].size;
                        // self.free[r]の削除
                        // free[r]がなくなったため前へ詰める
                        self.frees -= 1;
                        for j in r..self.frees as usize {
                            self.free[j] = self.free[j+1]   // 構造体の代入
                        }
                    }
                }
                return 0;   // 成功
            }
        }

        // 前とはまとめられなかった
        if r < self.frees as usize {
            // 後ろがある
            if addr + size == self.free[r].addr {
                // 後ろとはまとめられる
                self.free[r].addr = addr;
                self.free[r].size += size;
                return 0;   // 成功終了
            }
        }

        // 前にも後ろにもまとめられない
        if self.frees < MEMMAN_FREES {
            // free[i]より後ろを，後ろへずらして隙間を作る
            let mut j = self.frees as usize;
            while j > r {
                self.free[j] = self.free[j - 1];
                j -= 1;
            }
            self.frees += 1;
            if self.maxfrees < self.frees {
                self.maxfrees = self.frees; // 最大値を更新
            }
            self.free[r].addr = addr;
            self.free[r].size = size;
            return 0;
        }
        // 後ろにずらせなかった
        self.losts += 1;
        self.lostsize += size;
        return -1;  // 失敗
    }
}

pub fn memtest(start: u32, end: u32) -> u32 {
    let mut flg486: u8 = 0;

    // 386か486以降なのかを確認
    let mut eflg: u32 = load_eflags() as u32;
    eflg |= EFLAGS_AC_BIT;  // AC-bit = 1
    store_eflags(eflg as i32);
    eflg = load_eflags() as u32;
    if ((eflg & EFLAGS_AC_BIT) != 0) {
        // 386ではAC=1にしても自動で0に戻ってしまう
        flg486 = 1;
    }
    eflg &= !EFLAGS_AC_BIT;    // AC-bit = 0
    store_eflags(eflg as i32);

    if flg486 != 0 {
        let mut cr0: u32= load_cr0();
        cr0 |= CR0_CACHE_DISABLE;   // キャッシュ禁止
        store_cr0(cr0);
    }

    let i: u32 = memtest_sub(start, end);

    if flg486 != 0 {
        let mut cr0: u32 = load_cr0();
        cr0 &= !CR0_CACHE_DISABLE;
        store_cr0(cr0);
    }

    i
}

use volatile::Volatile;

fn memtest_sub(start: u32, end: u32) -> u32 {
    let pat0: u32 = 0xaa55aa55;
    let pat1: u32 = 0x55aa55aa;
    let mut r = start;
    for i in (start..end).step_by(0x1000) {
        r = i;
        let mp = (i + 0xffc) as *mut u32;
        let p = unsafe { &mut *(mp as *mut Volatile<u32>) };
        let old = p.read();
        p.write(pat0);
        p.write(p.read() ^ 0xffffffff);
        if p.read() != pat1 {
            p.write(old);
            break;
        }

        p.write(p.read() ^ 0xffffffff);
        if p.read() != pat0 {
            p.write(old);
            break;
        }

        p.write(old);
    }
    r  
}

// メモリのないところへのアクセスのため，したのコートでは
// コンパイラによって最適化され意図した処理が行われない．

// pub fn memtest_sub(start: u32, end: u32) -> u32 {
//     let pat0: u32 = 0xaa55aa55;
//     let pat1: u32 = 0x55aa55aa;
//     let mut r = start;
//     for i in (start..end).step_by(0x1000) {
//         r = i;
//         unsafe {
//             let p = (i + 0xffc) as *mut u32;
//             let old = *p;
//             *p = pat0;
//             *p ^= 0xffffffff;
//             if *p != pat1 {
//                 *p = old;
//                 break;
//             }

//             *p ^= 0xffffffff;
//             if *p != pat0 {
//                 *p = old;
//                 break;
//             }

//             *p = old;
//         }
//     }
//     r
// }