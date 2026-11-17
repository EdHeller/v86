// virtio-rng (src/virtio_rng.js): every buffer the driver offers on its queue is filled with random bytes, from a
// ChaCha20 stream keyed with the host's random numbers (crypto.getRandomValues) when the device is set up

use crate::cpu::virtio_dev::*;

struct ChaCha {
    key: [u32; 8],
    counter: u64,
    block: [u8; 64],
    used: usize,
}

static mut RNG: ChaCha = ChaCha { key: [0; 8], counter: 0, block: [0; 64], used: 64 };

fn quarter(s: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(16);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(12);
    s[a] = s[a].wrapping_add(s[b]);
    s[d] = (s[d] ^ s[a]).rotate_left(8);
    s[c] = s[c].wrapping_add(s[d]);
    s[b] = (s[b] ^ s[c]).rotate_left(7);
}

impl ChaCha {
    fn refill(&mut self) {
        let mut s = [0u32; 16];
        s[..4].copy_from_slice(&[0x61707865, 0x3320646e, 0x79622d32, 0x6b206574]);
        s[4..12].copy_from_slice(&self.key);
        s[12] = self.counter as u32;
        s[13] = (self.counter >> 32) as u32;
        let input = s;
        for _ in 0..10 {
            quarter(&mut s, 0, 4, 8, 12);
            quarter(&mut s, 1, 5, 9, 13);
            quarter(&mut s, 2, 6, 10, 14);
            quarter(&mut s, 3, 7, 11, 15);
            quarter(&mut s, 0, 5, 10, 15);
            quarter(&mut s, 1, 6, 11, 12);
            quarter(&mut s, 2, 7, 8, 13);
            quarter(&mut s, 3, 4, 9, 14);
        }
        for i in 0..16 {
            self.block[4 * i..4 * i + 4].copy_from_slice(&s[i].wrapping_add(input[i]).to_le_bytes());
        }
        self.counter = self.counter.wrapping_add(1);
        self.used = 0;
    }
    fn fill(&mut self, out: &mut [u8]) {
        for b in out.iter_mut() {
            if self.used == 64 {
                self.refill();
            }
            *b = self.block[self.used];
            self.used += 1;
        }
    }
}

/// The device's model is here: its key (eight words from the host's random numbers)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn virtio_rng_attach(d: i32, k0: u32, k1: u32, k2: u32, k3: u32, k4: u32, k5: u32, k6: u32, k7: u32) {
    RNG.key = [k0, k1, k2, k3, k4, k5, k6, k7];
    RNG.counter = 0;
    RNG.used = 64;
    MODEL[d as usize] = MODEL_RNG;
}

#[allow(static_mut_refs)]
pub unsafe fn fill(d: usize, queue: usize) {
    let mut bytes = [0u8; 4096];
    while has_request(d, queue) {
        let mut c = pop(d, queue);
        let mut left = c.length_writable as usize;
        while left > 0 {
            let n = left.min(bytes.len());
            RNG.fill(&mut bytes[..n]);
            if c.write(&bytes[..n]) == 0 {
                break;
            }
            left -= n;
        }
        push(d, queue, &c);
    }
    flush(d, queue);
}
