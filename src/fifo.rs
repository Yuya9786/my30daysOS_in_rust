pub struct FIFO8 {
    pub buf: [u8; 128],
    pub p: i32,
    pub q: i32,
    pub size: i32,
    pub free: i32,
    pub flags: i32,
}

const FLAGS_OVERRUN: i32 = 0x0001;

impl FIFO8 {
    pub fn new(size: i32) -> FIFO8 {
        FIFO8 {
            buf: [0; 128],
            p: 0,           // 書き込み位置
            q: 0,           // 読み込み位置
            size: size,
            free: size,     // 空き
            flags: 0,
        }
    }

    pub fn put(&mut self, data: u8) -> Result<(), &'static str> {
        if self.free == 0 {
            // 空きがなくて溢れた
            self.flags = self.flags | FLAGS_OVERRUN;
            return Err("FLAGS_OVERRUN ERROR");
        }
        self.buf[self.p as usize] = data;
        self.p += 1;
        if self.p >= self.size {
            self.p = 0;
        }
        self.free -= 1;
        return Ok(());
    }

    pub fn get(&mut self) -> Result<u8, &'static str> {
        if self.free == self.size {
            // バッファが空だった
            return Err("BUFFER EMPTY");
        }
        let data = self.buf[self.q as usize];
        self.q += 1;
        if self.q >= self.size {
            self.q = 0;
        }
        self.free += 1;
        return Ok(data)
    }

    pub fn status(&self) -> i32 {
        // データ数の報告
        self.size - self.free
    }
}