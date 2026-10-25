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
