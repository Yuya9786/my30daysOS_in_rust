use crate::binfo;
use crate::vga::{Color};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetFlag {
    AVAILABLE,
    USED,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SHEET {
    pub buf: usize,
    pub bxsize: usize,
    pub bysize: usize,
    pub vx0: i32,
    pub vy0: i32,
    pub col_inv: Option<Color>,   // 透明色番号
    pub height: Option<usize>,
    pub flags: SheetFlag,
}

impl SHEET {
    pub fn new() -> SHEET {
        SHEET {
            buf: 0,
            bxsize: 0,
            bysize: 0,
            vx0: 0,
            vy0: 0,
            col_inv: Some(Color::Black),
            height: None,
            flags: SheetFlag::AVAILABLE,   // 未使用マーク
        }
    }

    pub fn setbuf(&mut self, buf: usize, xsize: usize, ysize: usize, col_inv: Option<Color>) {
        self.buf = buf;
        self.bxsize = xsize;
        self.bysize = ysize;
        self.col_inv = col_inv;
    }
}

const MAX_SHEETS: usize = 256;

pub struct SHTCTL {
    pub vram: usize,
    pub map: usize,
    pub xsize: i32,
    pub ysize: i32,
    pub top: Option<usize>,
    pub sheets: [usize; MAX_SHEETS],
    // sheets_data上のindexを保持(Rustでは参照を持たせるのは取り回しが面倒なため)
    pub sheets_data: [SHEET; MAX_SHEETS], 
}

impl SHTCTL {
    pub fn new(map: usize) -> SHTCTL {
        SHTCTL {
            vram: binfo.vram,
            map: map,
            xsize: binfo.scrnx as i32,
            ysize: binfo.scrny as i32,
            top: None,
            sheets: [0; MAX_SHEETS],
            sheets_data: [SHEET::new(); MAX_SHEETS],
        }
    }

    pub fn alloc(&mut self) -> Option<usize> {
        // 未使用のSHEETを持ってくる 戻り値はSHTCTL.sheetsのindex
        for i in 0..MAX_SHEETS {
            if self.sheets_data[i].flags == SheetFlag::AVAILABLE {
                self.sheets_data[i].flags = SheetFlag::USED;      // 使用中マーク        
                self.sheets_data[i].height = None;    // 非表示中
                return Some(i)
            }
        }
        None
    }

    pub fn updown(&mut self, sheet_index: usize, height: Option<usize>) {
        let old = self.sheets_data[sheet_index].height; // 設定前の高さを記憶する

        // topより高い場合修正
        let hight = if let Some(h) = height {
            Some(core::cmp::min(
                if let Some(top) = self.top {
                    top as usize + 1
                } else {
                    0
                },
                h
            ))
        } else {
            None
        };
        
        self.sheets_data[sheet_index].height = height;  // 高さを設定

        // 以下は主にsheets[]の並び替え
        let mut a: usize = 0;
        let mut b: usize = 0;
        if let Some(old) = old {
            if let Some(height) = height {
                if old > height {   // 以前よりも低くなる
                    // 間のものを引き上げる
                    let mut h = old;
                    while h > height {
                        self.sheets[h] = self.sheets[h-1];
                        self.sheets_data[self.sheets[h]].height = Some(h);
                        h -= 1;
                    }
                    self.sheets[height] = sheet_index;
                    a = height;
                    b = old;
                } else if old < height {    // 以前よりも高くなる
                    // 間のものを押し下げる
                    let mut h = old;
                    while h < height {
                        self.sheets[h] = self.sheets[h+1];
                        self.sheets_data[self.sheets[h]].height = Some(h);
                        h += 1;
                    }
                    self.sheets[height] = sheet_index;
                    a = height;
                    b = height;
                }
            } else {    // 非表示化
                if let Some(top) = self.top {
                    if top > old {
                        // 上のものをおろす
                        let mut h = old;
                        while h < top {
                            self.sheets[h] = self.sheets[h+1];
                            self.sheets_data[self.sheets[h]].height = Some(h);
                            h += 1;
                        }
                    }
                    self.top = if top > 0 { // 表示中のものが一つ減るので, 一番上の高さが減る
                        Some(top - 1)
                    } else {
                        None
                    };
                }
                a = 0;
                b = old - 1;
            }
        } else {    // 非表示から表示状態へ
            // 上になるものを持ち上げる
            if let Some(height) = height {
                if let Some(top) = self.top {
                    let mut h = top;
                    while h >= height {
                        self.sheets[h+1] = self.sheets[h];
                        self.sheets_data[self.sheets[h+1]].height = Some(h+1);
                        h -= 1;
                    }
                }
                self.sheets[height] = sheet_index;
                if let Some(top) = self.top {   // 表示する下敷きが1枚増えるので，一番上の高さが増える
                    self.top = Some(top+1);
                } else {
                    self.top = Some(0);
                }
                a = height;
                b = height;
            } else {
                return;
            }
        }

        let sht = self.sheets_data[sheet_index];
        self.refreshmap(sht.vx0, sht.vy0, sht.vx0 + sht.bxsize as i32, sht.vy0 + sht.bysize as i32, a);
        self.refreshsub(sht.vx0, sht.vy0, sht.vx0 + sht.bxsize as i32, sht.vy0 + sht.bysize as i32, a, b);     // 
    }

    pub fn refresh(&mut self, sheet_index: usize, bx0: i32, by0: i32, bx1: i32, by1: i32) {
        if let Some(height) = self.sheets_data[sheet_index].height {
            let sht = &self.sheets_data[sheet_index];
            self.refreshsub(sht.vx0 + bx0, sht.vy0 + by0, sht.vx0 + bx1, sht.vy0 + by1, height, height);
        }
    }

    pub fn refreshsub(&mut self, vx0: i32, vy0: i32, vx1: i32, vy1: i32, h0: usize, h1: usize) {
        let vx0 = core::cmp::max(0, vx0);
        let vy0 = core::cmp::max(0, vy0);
        let vx1 = core::cmp::min(vx1, self.xsize as i32);
        let vy1 = core::cmp::min(vy1, self.ysize as i32);
        if let Some(top) = self.top {
            for h in h0..=h1 {
                let sid = self.sheets[h];
                let sht = &self.sheets_data[self.sheets[h]];
                let buf = sht.buf;
                let bx0 = if vx0 > sht.vx0 { vx0 - sht.vx0 } else { 0 } as usize;
                let by0 = if vy0 > sht.vy0 { vy0 - sht.vy0 } else { 0 } as usize;
                let bx1 = if vx1 > sht.vx0 {
                    core::cmp::min(vx1 - sht.vx0, sht.bxsize as i32)
                } else {
                    0
                } as usize;
                let by1 = if vy1 > sht.vy0 {
                    core::cmp::min(vy1 - sht.vy0, sht.bysize as i32)
                } else {
                    0
                } as usize;
                for by in by0..by1 {
                    let vy = sht.vy0 + by as i32;
                    for bx in bx0..bx1 {
                        let vx = sht.vx0 + bx as i32;
                        let map_sid = unsafe {
                            *((self.map as *mut u8).offset((vy * self.xsize + vx) as isize))
                        };
                        if sid == map_sid as usize {
                            let c = unsafe { *((buf + by * sht.bxsize + bx) as *const Color) };
                            unsafe {
                                *((self.vram as *mut u8).offset((vy * self.xsize + vx) as isize)) = c as u8;
                            }
                        }
                    }
                }
            }
        }
    }

    pub fn slide(&mut self, sheet_index: usize, x: i32, y: i32) {
        let sht = &mut self.sheets_data[sheet_index];
        let old_vx0 = sht.vx0;
        let old_vy0 = sht.vy0;
        sht.vx0 = x;
        sht.vy0 = y;
        if let Some(h) = sht.height {    // もしも表示中なら
            let bxsize = sht.bxsize as i32;
            let bysize = sht.bysize as i32;
            self.refreshmap(old_vx0, old_vy0, old_vx0 + bxsize, old_vy0 + bysize, 0);
            self.refreshmap(x, y, x + bxsize, y + bysize, h);
            self.refreshsub(old_vx0, old_vy0, old_vx0 + bxsize, old_vy0 + bysize, 0, h - 1);
            self.refreshsub(x, y, x + bxsize, y + bysize, h, h);
        }
    }

    pub fn free(&mut self, sheet_index: usize) {
        if let Some(h) = self.sheets_data[sheet_index].height {
            self.updown(sheet_index, None);   // 表示中なら非表示にする
        }
        self.sheets_data[sheet_index].flags = SheetFlag::AVAILABLE; // 未使用マーク
    }

    pub fn refreshmap(&mut self, vx0: i32, vy0: i32, vx1: i32, vy1: i32, h0: usize) {
        if self.top.is_none() {
            return;
        }

        let vx0 = core::cmp::max(0, vx0);
        let vy0 = core::cmp::max(0, vy0);
        let vx1 = core::cmp::min(vx1, self.xsize as i32);
        let vy1 = core::cmp::min(vy1, self.ysize as i32);

        for h in h0..=self.top.unwrap() {
            let sid = self.sheets[h];
            let sht = &self.sheets_data[self.sheets[h]];
            let buf = sht.buf;
            let bx0 = if vx0 > sht.vx0 { vx0 - sht.vx0 } else { 0 } as usize;
            let by0 = if vy0 > sht.vy0 { vy0 - sht.vy0 } else { 0 } as usize;
            let bx1 = if vx1 > sht.vx0 {
                core::cmp::min(vx1 - sht.vx0, sht.bxsize as i32)
            } else {
                0
            } as usize;
            let by1 = if vy1 > sht.vy0 {
                core::cmp::min(vy1 - sht.vy0, sht.bysize as i32)
            } else {
                0
            } as usize;
            for by in by0..by1 {
                let vy = sht.vy0 + by as i32;
                for bx in bx0..bx1 {
                    let vx = sht.vx0 + bx as i32;
                    let c = unsafe { *((buf + by * sht.bxsize + bx) as *const Color) };
                    if Some(c) != sht.col_inv {
                        unsafe {
                            *((self.map as *mut u8).offset((vy * self.xsize + vx) as isize)) = sid as u8;
                        }
                    }
                }
            }
        }        
    }
}