// The I/O port table's view from the core: for every port, which access sizes a device in JavaScript answers
// (src/io.js keeps it in step with its table). An access no device answers gets the empty port's value (all ones
// on reads, nothing on writes) without a call to JavaScript: the ISA plug-and-play ports that Linux probes, the
// unused ports of a range. Also port 0x92 (the fast A20 gate's register, which only holds what was written).

pub const READ8: u8 = 1;
pub const READ16: u8 = 2;
pub const READ32: u8 = 4;
pub const WRITE8: u8 = 8;
pub const WRITE16: u8 = 16;
pub const WRITE32: u8 = 32;
pub const READ_BLOCK: u8 = 64;
pub const WRITE_BLOCK: u8 = 128;

static mut FLAGS: [u8; 0x10000] = [0; 0x10000];
/// the table is filled (JavaScript's IO made); until then every access goes to JavaScript
static mut READY: bool = false;
static mut PORT_92: u8 = 0;

#[no_mangle]
pub fn io_ports_flags_addr() -> u32 { &raw mut FLAGS as u32 }

#[no_mangle]
pub unsafe fn io_ports_ready(ready: bool) { READY = ready; }

/// Whether a device in JavaScript answers this access (false only when the table says nobody does)
#[inline]
pub unsafe fn handled(port: i32, what: u8) -> bool {
    !READY || super::parallel::WORKER || FLAGS[port as usize & 0xFFFF] & what != 0
}

#[no_mangle]
pub unsafe fn port92_read() -> i32 { PORT_92 as i32 }

#[no_mangle]
pub unsafe fn port92_write(value: i32) { PORT_92 = value as u8; }

#[inline]
pub unsafe fn serves_92(port: i32) -> bool { port == 0x92 && READY && !super::parallel::WORKER }
