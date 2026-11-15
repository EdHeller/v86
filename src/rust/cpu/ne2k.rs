// NE2000 (RTL8029 on PCI) network card: the register pages, the 32 KiB buffer memory and its remote DMA (the data
// port). Was src/ne2k.js, which keeps transmitting (a bus message with the frame) and receiving (frames from the
// host's network into the ring), the MAC address translation of snapshots and the snapshot format, on the state
// here. Machines with the Q35 chipset (its interrupt routing is JavaScript's) run the device in JavaScript.

const E8390_CMD: i32 = 0x00;
const NE_DATAPORT: i32 = 0x10;
const NE_RESET: i32 = 0x1f;

const ENISR_RDC: i32 = 0x40;
const ENISR_RESET: i32 = 0x80;

const START_PAGE: i32 = 0x40;
const STOP_PAGE: i32 = 0x80;
const MEMORY_SIZE: usize = 256 * 0x80;

// (the layout is read by src/ne2k.js)
#[repr(C)]
pub struct Ne2k {
    isr: i32,      // 0
    imr: i32,      // 1
    cr: i32,       // 2
    dcfg: i32,     // 3
    rcnt: i32,     // 4
    tcnt: i32,     // 5
    tpsr: i32,     // 6
    rsar: i32,     // 7
    pstart: i32,   // 8
    curpg: i32,    // 9
    boundary: i32, // 10
    pstop: i32,    // 11
    rxcr: i32,     // 12
    txcr: i32,     // 13
    tsr: i32,      // 14
    pci_id: i32,   // 15
    port: i32,     // 16
    present: i32,  // 17
    mac: [u8; 6],  // 72
    mar: [u8; 8],  // 78
    _pad: [u8; 2], // 86
    memory: [u8; MEMORY_SIZE], // 88
}

static mut NE2K: Ne2k = Ne2k {
    isr: 0,
    imr: 0,
    cr: 1,
    dcfg: 0,
    rcnt: 0,
    tcnt: 0,
    tpsr: 0,
    rsar: 0,
    pstart: START_PAGE,
    curpg: 0,
    boundary: 0,
    pstop: STOP_PAGE,
    rxcr: 0,
    txcr: 0,
    tsr: 1,
    pci_id: 0,
    port: 0,
    present: 0,
    mac: [0; 6],
    mar: [0xFF; 8],
    _pad: [0; 2],
    memory: [0; MEMORY_SIZE],
};

#[allow(static_mut_refs)]
fn ne2k() -> &'static mut Ne2k { unsafe { &mut NE2K } }

#[no_mangle]
pub fn get_ne2k_addr() -> u32 { &raw mut NE2K as u32 }

/// The first card (id 0); its fields are then set from JavaScript
#[no_mangle]
pub unsafe fn ne2k_init(port: i32, pci_id: i32) {
    let n = ne2k();
    n.memory.fill(0);
    n.mar = [0xFF; 8];
    n.port = port;
    n.pci_id = pci_id;
    n.present = 1;
}

impl Ne2k {
    fn page(&self) -> i32 { self.cr >> 6 & 3 }

    unsafe fn do_interrupt(&mut self, mask: i32) {
        self.isr |= mask;
        self.update_irq();
    }

    unsafe fn update_irq(&mut self) { super::pci::set_irq(self.pci_id as usize & 0xFF, self.imr & self.isr != 0); }

    fn advance(&mut self) {
        self.rsar += 1;
        self.rcnt -= 1;
        if self.rsar >= self.pstop << 8 {
            self.rsar += (self.pstart - self.pstop) << 8;
        }
    }

    unsafe fn data_write(&mut self, byte: i32) {
        if self.rsar <= 0x10 || self.rsar >= START_PAGE << 8 && self.rsar < STOP_PAGE << 8 {
            self.memory[self.rsar as usize] = byte as u8;
        }
        self.advance();
        if self.rcnt == 0 {
            self.do_interrupt(ENISR_RDC);
        }
    }

    unsafe fn data_read(&mut self) -> i32 {
        let data = if self.rsar >= 0 && self.rsar < STOP_PAGE << 8 { self.memory[self.rsar as usize] as i32 } else { 0 };
        self.advance();
        if self.rcnt == 0 {
            self.do_interrupt(ENISR_RDC);
        }
        data
    }

    unsafe fn read(&mut self, reg: i32) -> i32 {
        let pg = self.page();
        match reg {
            E8390_CMD => self.cr,
            0x01 => match pg { 0 | 2 => self.pstart, 1 => self.mac[0] as i32, _ => 0 },
            0x02 => match pg { 0 | 2 => self.pstop, 1 => self.mac[1] as i32, _ => 0 },
            0x03 => match pg { 0 => self.boundary, 1 => self.mac[2] as i32, _ => 0 },
            0x04 => match pg { 0 => self.tsr, 1 => self.mac[3] as i32, _ => 0 },
            0x05 => match pg { 1 => self.mac[4] as i32, _ => 0 },
            0x06 => match pg { 1 => self.mac[5] as i32, _ => 0 },
            0x07 => match pg { 0 => self.isr, 1 => self.curpg, _ => 0 },
            0x08 => match pg { 0 => self.rsar & 0xFF, 1 => self.mar[0] as i32, _ => 0 },
            0x09 => match pg { 0 => self.rsar >> 8 & 0xFF, 1 => self.mar[1] as i32, _ => 0 },
            0x0a => match pg { 0 => 0x50, 1 => self.mar[2] as i32, _ => 0 },
            0x0b => match pg { 0 => 0x43, 1 => self.mar[3] as i32, _ => 0 },
            // receive status ok
            0x0c => match pg { 0 => 1 | 1 << 3, 1 => self.mar[4] as i32, _ => 0 },
            0x0d => match pg { 1 => self.mar[5] as i32, _ => 0 },
            0x0e => match pg { 1 => self.mar[6] as i32, _ => 0 },
            0x0f => match pg { 1 => self.mar[7] as i32, _ => 0 },
            NE_RESET => {
                self.do_interrupt(ENISR_RESET);
                0
            },
            _ => 0xFF,
        }
    }

    unsafe fn write(&mut self, reg: i32, data: i32) {
        let pg = self.page();
        let data = data & 0xFF;
        match reg {
            E8390_CMD => {
                // (a command with transmit is JavaScript's: see serves)
                self.cr = data;
                if self.cr & 1 != 0 {
                    return;
                }
                if data & 0x18 != 0 && self.rcnt == 0 {
                    self.do_interrupt(ENISR_RDC);
                }
            },
            0x01 => match pg { 0 => self.pstart = data, 1 => self.mac[0] = data as u8, _ => {} },
            0x02 => match pg {
                0 => self.pstop = data.min((MEMORY_SIZE >> 8) as i32),
                1 => self.mac[1] = data as u8,
                _ => {},
            },
            0x03 => match pg { 0 => self.boundary = data, 1 => self.mac[2] = data as u8, _ => {} },
            0x04 => match pg { 0 => self.tpsr = data, 1 => self.mac[3] = data as u8, _ => {} },
            0x05 => match pg { 0 => self.tcnt = self.tcnt & !0xFF | data, 1 => self.mac[4] = data as u8, _ => {} },
            0x06 => match pg { 0 => self.tcnt = self.tcnt & 0xFF | data << 8, 1 => self.mac[5] = data as u8, _ => {} },
            0x07 => match pg {
                0 => {
                    self.isr &= !data;
                    self.update_irq();
                },
                1 => self.curpg = data,
                _ => {},
            },
            0x08 => if pg == 0 { self.rsar = self.rsar & 0xFF00 | data },
            0x09 => if pg == 0 { self.rsar = self.rsar & 0xFF | data << 8 & 0xFF00 },
            0x0a => if pg == 0 { self.rcnt = self.rcnt & 0xFF00 | data },
            0x0b => if pg == 0 { self.rcnt = self.rcnt & 0xFF | data << 8 & 0xFF00 },
            0x0c => if pg == 0 { self.rxcr = data },
            0x0d => if pg == 0 { self.txcr = data },
            0x0e => if pg == 0 { self.dcfg = data },
            0x0f => if pg == 0 {
                self.imr = data;
                self.update_irq();
            },
            _ => {},
        }
    }
}

/// the register of a port of the card, if it is one
#[inline]
unsafe fn register(port: i32) -> Option<i32> {
    let n = ne2k();
    if n.present == 0 || super::parallel::WORKER || super::pci::is_q35() {
        return None;
    }
    // (the card answers at its ISA port, and at its BAR when the BIOS moved it above 0x1000, as src/pci.js does)
    let bar = super::pci::io_bar0(n.pci_id as usize & 0xFF);
    let base = if port & !0x1F == n.port { n.port } else if bar >= 0x1000 && port & !0x1F == bar { bar } else { return None };
    let reg = port - base;
    if reg <= 0x0F || reg == NE_DATAPORT || reg == NE_RESET { Some(reg) } else { None }
}

/// Whether the core answers this access itself. A command that transmits is JavaScript's (a bus message)
#[inline]
pub unsafe fn serves(port: i32, size: i32, value: i32, write: bool) -> bool {
    match register(port) {
        Some(NE_DATAPORT) => true,
        Some(E8390_CMD) if write => size == 8 && (value & 1 != 0 || value & 4 == 0),
        Some(_) => size == 8,
        None => false,
    }
}

#[no_mangle]
pub unsafe fn ne2k_port_read(port: i32, size: i32) -> i32 {
    let Some(reg) = register(port) else { return -1 };
    let n = ne2k();
    if reg != NE_DATAPORT {
        return n.read(reg);
    }
    match size {
        8 => {
            let v = if n.dcfg & 1 != 0 { n.data_read() | n.data_read() << 8 } else { n.data_read() };
            v & 0xFF
        },
        16 => if n.dcfg & 1 != 0 { n.data_read() | n.data_read() << 8 } else { n.data_read() },
        _ => n.data_read() | n.data_read() << 8 | n.data_read() << 16 | n.data_read() << 24,
    }
}

#[no_mangle]
pub unsafe fn ne2k_port_write(port: i32, size: i32, value: i32) {
    let Some(reg) = register(port) else { return };
    let n = ne2k();
    if reg != NE_DATAPORT {
        return n.write(reg, value);
    }
    match size {
        // (8-bit writes go the 16-bit way, as in JavaScript)
        8 | 16 => {
            n.data_write(value & 0xFF);
            if n.dcfg & 1 != 0 {
                n.data_write(value >> 8 & 0xFF);
            }
        },
        _ => {
            for i in 0..4 {
                n.data_write(value >> (8 * i) & 0xFF);
            }
        },
    }
}
