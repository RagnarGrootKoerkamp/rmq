// AI Generated

pub struct PackedVec {
    words: Vec<u64>,
    width: u32, // bits per element, 1..=64
    mask: u64,
    len: usize,
}

impl PackedVec {
    pub fn new(len: usize, width: u32) -> Self {
        assert!((1..=64).contains(&width));
        let bits = len * width as usize;
        // One extra word of padding so get/set can always touch words[w + 1].
        let words = vec![0; bits.div_ceil(64) + 1];
        let mask = if width == 64 { u64::MAX } else { (1 << width) - 1 };
        Self { words, width, mask, len }
    }

    #[inline]
    fn locate(&self, i: usize) -> (usize, u32) {
        debug_assert!(i < self.len);
        let bit = i * self.width as usize;
        (bit / 64, (bit % 64) as u32)
    }

    #[inline]
    pub fn get(&self, i: usize) -> u64 {
        let (w, s) = self.locate(i);
        let x = self.words[w] as u128 | (self.words[w + 1] as u128) << 64;
        (x >> s) as u64 & self.mask
    }

    #[inline]
    pub fn set(&mut self, i: usize, v: u64) {
        debug_assert!(v <= self.mask);
        let (w, s) = self.locate(i);
        let mut x = self.words[w] as u128 | (self.words[w + 1] as u128) << 64;
        x &= !((self.mask as u128) << s);
        x |= (v as u128) << s;
        self.words[w] = x as u64;
        self.words[w + 1] = (x >> 64) as u64;
    }

    pub fn len(&self) -> usize {
        self.len
    }
}
