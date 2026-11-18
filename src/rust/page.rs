use std::hash::{BuildHasherDefault, Hasher};
use std::ops::RangeInclusive;

/// Cheap hasher for the page-keyed maps of the jit, which are consulted on every memory write
#[derive(Default, Clone, Copy)]
pub struct PageHasher(u64);
impl Hasher for PageHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.0 = (self.0 << 8 | b as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
        }
    }
    fn write_u32(&mut self, i: u32) { self.0 = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15); }
    fn finish(&self) -> u64 { self.0 >> 20 | self.0 << 44 }
}
pub type PageBuildHasher = BuildHasherDefault<PageHasher>;

/// Fast hasher for the jit's per-compile maps and sets (addresses, labels, table indices): FxHash's step, mixed at the
/// end so that keys aligned to large powers of two spread over the table's low bits. std's default (SipHash with a
/// random seed) was a visible part of compiling
#[derive(Default, Clone, Copy)]
pub struct FastHasher(u64);
const FAST_SEED: u64 = 0x51_7C_C1_B7_27_22_0A_95;
impl FastHasher {
    #[inline(always)]
    fn add(&mut self, i: u64) { self.0 = (self.0.rotate_left(5) ^ i).wrapping_mul(FAST_SEED); }
}
impl Hasher for FastHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.add(b as u64);
        }
    }
    fn write_u8(&mut self, i: u8) { self.add(i as u64) }
    fn write_u16(&mut self, i: u16) { self.add(i as u64) }
    fn write_u32(&mut self, i: u32) { self.add(i as u64) }
    fn write_u64(&mut self, i: u64) { self.add(i) }
    fn write_usize(&mut self, i: usize) { self.add(i as u64) }
    fn write_i32(&mut self, i: i32) { self.add(i as u32 as u64) }
    fn finish(&self) -> u64 { self.0 ^ self.0 >> 29 ^ self.0 >> 47 }
}
pub type FastBuildHasher = BuildHasherDefault<FastHasher>;

#[derive(Copy, Clone, Eq, Hash, PartialEq)]
pub struct Page(u32);
impl Page {
    pub fn page_of(address: u32) -> Page { Page(address >> 12) }
    pub fn to_address(self) -> u32 { self.0 << 12 }

    pub fn to_u32(self) -> u32 { self.0 }
    pub fn of_u32(page: u32) -> Page { Page(page) }

    pub fn address_range(self) -> RangeInclusive<u32> {
        self.to_address()..=self.to_address() + 4095
    }
}
