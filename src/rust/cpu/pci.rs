// PCI configuration mechanism #1 (ports 0xCF8 and 0xCFC) and the configuration spaces of every function. Was
// src/pci.js, which keeps registering devices, the writes that move or size BARs and the ROM, the PAM registers,
// the hooks of plugin devices, the reset sequence of 0xCF9, interrupt routing and the snapshot format; it works on
// the spaces in this module's memory. http://wiki.osdev.org/PCI

// (the layout is read by src/pci.js)
#[repr(C)]
pub struct Pci {
    /// the 256 functions of bus 0, 64 dwords each
    spaces: [[i32; 64]; 256], // 0
    addr: [u8; 4],            // 65536
    response: [u8; 4],        // 65540
    status: [u8; 4],          // 65544
    /// per function: 1 present, 2 its configuration writes go through JavaScript
    flags: [u8; 256], // 65548
    present: u32,     // 65804
}

/// (debugging: every access through JavaScript)
pub static mut PCI_JS_ONLY: bool = false;
pub const FLAG_PRESENT: u8 = 1;
pub const FLAG_JS_WRITES: u8 = 2;

static mut PCI: Pci = Pci { spaces: [[0; 64]; 256], addr: [0; 4], response: [0; 4], status: [0; 4], flags: [0; 256], present: 0 };

#[allow(static_mut_refs)]
fn pci() -> &'static mut Pci { unsafe { &mut PCI } }

#[no_mangle]
pub fn get_pci_addr() -> u32 { &raw mut PCI as u32 }

#[no_mangle]
pub unsafe fn pci_init() {
    let p = pci();
    p.spaces = [[0; 64]; 256];
    p.addr = [0; 4];
    p.response = [0; 4];
    p.status = [0; 4];
    p.flags = [0; 256];
    p.present = 1;
}

#[no_mangle]
pub unsafe fn pci_set_flags(bdf: i32, flags: i32) { pci().flags[bdf as usize & 0xFF] = flags as u8; }

impl Pci {
    fn query(&mut self) {
        // Bit | .31                     .0
        // Fmt | EBBBBBBBBDDDDDFFFRRRRRR00
        let bdf = (self.addr[2] as usize) << 8 | self.addr[1] as usize;
        let offset = (self.addr[0] & 0xFC) as usize;
        if bdf < 256 && self.flags[bdf] & FLAG_PRESENT != 0 {
            self.status = 0x80000000u32.to_le_bytes();
            self.response = self.spaces[bdf][offset >> 2].to_le_bytes();
        }
        else {
            self.response = [0xFF; 4];
            self.status = [0; 4];
        }
    }

    fn address(&self) -> u32 { u32::from_le_bytes(self.addr) }

    /// a byte of the address register (0xCF8 + i)
    fn write_addr_byte(&mut self, i: usize, value: u8) {
        match i {
            0 => self.addr[0] = value & 0xFC,
            3 => {
                self.addr[3] = value;
                self.query();
            },
            _ => self.addr[i] = value,
        }
    }
}

/// Whether the core answers this access itself
#[inline]
pub unsafe fn serves(port: i32, size: i32, value: i32, write: bool) -> bool {
    let p = pci();
    if PCI_JS_ONLY || p.present == 0 || super::parallel::WORKER || !(0xCF8..=0xCFF).contains(&port) {
        return false;
    }
    let off = port & 3;
    // (the consecutive registers: 16-bit accesses at 0 and 2, 32-bit ones at 0)
    if size == 16 && off & 1 != 0 || size == 32 && off != 0 {
        return false;
    }
    if !write {
        return true;
    }
    if port < 0xCFC {
        // the reset sequence through 0xCF9 is JavaScript's
        let touches_cf9 = off == 1 || off == 0 && size >= 16;
        if touches_cf9 {
            let new = if off == 1 { value } else { value >> 8 } as u8;
            if p.addr[1] & 0x06 == 0x02 && new & 0x06 == 0x06 {
                return false;
            }
        }
        return true;
    }
    // a data write: plain configuration bytes of a function without hooks
    let address = p.address().wrapping_add(off as u32);
    let bdf = (address >> 8 & 0xFFFF) as usize;
    let offset = address & 0xFF;
    if bdf >= 256 || p.flags[bdf] & FLAG_PRESENT == 0 {
        // (no function there: the write does nothing, as in JavaScript)
        return true;
    }
    p.flags[bdf] & FLAG_JS_WRITES == 0 && !(0x10..0x34).contains(&offset) && !(offset < 0x10 && offset + (size as u32 / 8) > 0x10)
}

#[no_mangle]
pub unsafe fn pci_port_read(port: i32, size: i32) -> i32 {
    let p = pci();
    let reg = if port < 0xCFC { p.status } else { p.response };
    let off = (port & 3) as usize;
    match size {
        8 => reg[off] as i32,
        16 => reg[off] as i32 | (reg[off + 1] as i32) << 8,
        _ => i32::from_le_bytes(reg),
    }
}

#[no_mangle]
pub unsafe fn pci_port_write(port: i32, size: i32, value: i32) {
    let p = pci();
    let off = (port & 3) as usize;
    let bytes = (size / 8) as usize;
    if port < 0xCFC {
        for i in 0..bytes {
            p.write_addr_byte(off + i, (value >> (8 * i)) as u8);
        }
        return;
    }
    let address = p.address().wrapping_add(off as u32);
    let bdf = (address >> 8 & 0xFFFF) as usize;
    if bdf >= 256 || p.flags[bdf] & FLAG_PRESENT == 0 {
        return;
    }
    let offset = (address & 0xFF) as usize;
    let space = &mut p.spaces[bdf];
    for i in 0..bytes {
        let at = offset + i;
        if at >= 256 {
            break;
        }
        let shift = (at & 3) * 8;
        let dword = &mut space[at >> 2];
        *dword = *dword & !(0xFF << shift) | ((value >> (8 * i)) & 0xFF) << shift;
    }
}
