// IDE (ATA) task file registers of the two channels: error, sector count, LBA, device, status and alternate status,
// features, and the drive selection. Was part of src/ide.js, which keeps the commands, the data transfers (PIO and
// bus master DMA), ATAPI and the snapshot format, and reads and writes these registers through accessors.

use crate::cpu::cpu::device_lower_irq;

const ATA_REG_ERROR: i32 = 1;
const ATA_REG_SECTOR: i32 = 2;
const ATA_REG_LBA_LOW: i32 = 3;
const ATA_REG_LBA_MID: i32 = 4;
const ATA_REG_LBA_HIGH: i32 = 5;
const ATA_REG_DEVICE: i32 = 6;
const ATA_REG_STATUS: i32 = 7;
const ATA_REG_FEATURES: i32 = 1;
const ATA_DR_DEV: i32 = 0x10;

// (the layout is read by src/ide.js: eleven 32-bit fields per interface)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Iface {
    status: i32,          // 0
    error: i32,           // 1
    sector_count: i32,    // 2
    lba_low: i32,         // 3
    lba_mid: i32,         // 4
    lba_high: i32,        // 5
    device: i32,          // 6
    features: i32,        // 7
    is_lba: i32,          // 8
    head: i32,            // 9
    drive_connected: i32, // 10
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Channel {
    iface: [Iface; 2],       // 0, 44
    current: i32,            // 88: 0 master, 1 slave
    device_control: i32,     // 92
    command_base: i32,       // 96
    control_base: i32,       // 100
    irq: i32,                // 104
    present: i32,            // 108
    // the bus master registers (BAR4: the primary channel's at +0, the secondary's at +8)
    prdt_addr: i32,          // 112
    dma_status: i32,         // 116
    dma_command: i32,        // 120
}

const IFACE0: Iface = Iface {
    status: 0,
    error: 0,
    sector_count: 0,
    lba_low: 0,
    lba_mid: 0,
    lba_high: 0,
    device: 0,
    features: 0,
    is_lba: 0,
    head: 0,
    drive_connected: 0,
};
const CHANNEL0: Channel =
    Channel {
        iface: [IFACE0; 2],
        current: 0,
        device_control: 0,
        command_base: -1,
        control_base: -1,
        irq: 0,
        present: 0,
        prdt_addr: 0,
        dma_status: 0,
        dma_command: 0,
    };

/// the bus master registers' fixed base and the controller's PCI function (whose BAR4 the BIOS may move)
static mut BUS_MASTER_BASE: i32 = -1;
static mut PCI_BDF: i32 = -1;

#[no_mangle]
pub unsafe fn ide_set_bus_master(base: i32, pci_bdf: i32) {
    BUS_MASTER_BASE = base;
    PCI_BDF = pci_bdf;
}

/// (channel, register 0..7) of a bus master port, at the fixed base or at BAR4 when the BIOS moved it above 0x1000
#[inline]
unsafe fn bus_master(port: i32) -> Option<(usize, i32)> {
    if BUS_MASTER_BASE < 0 {
        return None;
    }
    let bar = if PCI_BDF >= 0 { super::pci::io_bar(PCI_BDF as usize & 0xFF, 4) } else { -1 };
    let base = if port & !0xF == BUS_MASTER_BASE { BUS_MASTER_BASE } else if bar >= 0x1000 && port & !0xF == bar { bar } else { return None };
    let n = ((port - base) >> 3) as usize;
    if channel(n).present == 0 {
        return None;
    }
    Some((n, (port - base) & 7))
}

/// Whether the core answers this bus master or device control access itself. Setting the start bit starts a
/// transfer (JavaScript's); a device control write with the soft reset bit resets the drives (JavaScript's)
#[inline]
pub unsafe fn bus_master_serves(port: i32, size: i32, value: i32, write: bool) -> bool {
    if super::parallel::WORKER {
        return false;
    }
    if write && size == 8 {
        for n in 0..2 {
            let c = channel(n);
            if c.present != 0 && port == c.control_base {
                return value & 4 == 0;
            }
        }
    }
    let Some((n, reg)) = bus_master(port) else { return false };
    let c = channel(n);
    match (reg, size, write) {
        (0, 8, false) | (0, 32, false) | (2, 8, false) | (2, 8, true) | (4, 32, false) | (4, 32, true) => true,
        // (the start bit set where it was clear: the transfer begins)
        (0, 8, true) | (0, 32, true) => !(value & 1 != 0 && c.dma_command & 1 == 0),
        _ => false,
    }
}

#[no_mangle]
pub unsafe fn ide_bus_master_read(port: i32, size: i32) -> i32 {
    let Some((n, reg)) = bus_master(port) else { return -1 };
    let c = channel(n);
    match (reg, size) {
        (0, 32) => c.dma_command | c.dma_status << 16,
        (0, _) => c.dma_command,
        (2, _) => c.dma_status,
        (4, _) => c.prdt_addr,
        _ => -1,
    }
}

#[no_mangle]
pub unsafe fn ide_bus_master_write(port: i32, size: i32, value: i32) {
    for n in 0..2 {
        let c = channel(n);
        if c.present != 0 && port == c.control_base {
            // (without a soft reset: see bus_master_serves)
            c.device_control = value & 0xFF;
            return;
        }
    }
    let Some((n, reg)) = bus_master(port) else { return };
    let c = channel(n);
    let write_command = |c: &mut Channel, value: i32| {
        let old = c.dma_command;
        c.dma_command = value & 0x09;
        if old & 1 != value & 1 && value & 1 == 0 {
            c.dma_status &= !1;
        }
    };
    match (reg, size) {
        (0, 32) => {
            write_command(c, value & 0xFF);
            c.dma_status &= !(value >> 16 & 6);
        },
        (0, _) => write_command(c, value & 0xFF),
        (2, _) => c.dma_status &= !(value & 6),
        (4, _) => c.prdt_addr = value,
        _ => {},
    }
}

static mut CHANNELS: [Channel; 2] = [CHANNEL0; 2];

#[allow(static_mut_refs)]
unsafe fn channel(n: usize) -> &'static mut Channel { &mut CHANNELS[n] }

#[no_mangle]
pub unsafe fn get_ide_addr(channel_nr: i32) -> u32 { channel(channel_nr as usize & 1) as *mut Channel as u32 }

#[no_mangle]
pub unsafe fn ide_channel_init(channel_nr: i32, command_base: i32, control_base: i32, irq: i32) {
    let c = channel(channel_nr as usize & 1);
    *c = CHANNEL0;
    c.command_base = command_base;
    c.control_base = control_base;
    c.irq = irq;
    c.present = 1;
}

/// (channel, register) of a port: the register of the command block, or 8 for the alternate status register
#[inline]
unsafe fn decode(port: i32) -> Option<(usize, i32)> {
    for n in 0..2 {
        let c = channel(n);
        if c.present == 0 {
            continue;
        }
        if port & !7 == c.command_base {
            return Some((n, port & 7));
        }
        if port == c.control_base {
            return Some((n, 8));
        }
    }
    None
}

impl Channel {
    fn read_status(&self) -> i32 {
        let i = &self.iface[self.current as usize & 1];
        if i.drive_connected != 0 { i.status } else { 0 }
    }
}

/// Whether the core answers this 8-bit access itself: the task file registers. The data port, the command register
/// and the device control register go through JavaScript
#[inline]
pub unsafe fn serves(port: i32, write: bool) -> bool {
    if super::parallel::WORKER || !(0x170..=0x177).contains(&port) && !(0x1F0..=0x1F7).contains(&port) && port != 0x376 && port != 0x3F6 {
        return false;
    }
    match decode(port) {
        Some((_, reg)) => {
            if write {
                (ATA_REG_FEATURES..=ATA_REG_DEVICE).contains(&reg)
            }
            else {
                (ATA_REG_ERROR..=ATA_REG_STATUS).contains(&reg) || reg == 8
            }
        },
        None => false,
    }
}

#[no_mangle]
pub unsafe fn ide_port_read(port: i32) -> i32 {
    let Some((n, reg)) = decode(port) else { return 0xFF };
    let c = channel(n);
    let i = &c.iface[c.current as usize & 1];
    match reg {
        ATA_REG_ERROR => i.error & 0xFF,
        ATA_REG_SECTOR => i.sector_count & 0xFF,
        ATA_REG_LBA_LOW => i.lba_low & 0xFF,
        ATA_REG_LBA_MID => i.lba_mid & 0xFF,
        ATA_REG_LBA_HIGH => i.lba_high & 0xFF,
        ATA_REG_DEVICE => i.device & 0xFF,
        ATA_REG_STATUS => {
            let status = c.read_status();
            device_lower_irq(c.irq as u8);
            status
        },
        8 => c.read_status(),
        _ => 0xFF,
    }
}

#[no_mangle]
pub unsafe fn ide_port_write(port: i32, data: i32) {
    let Some((n, reg)) = decode(port) else { return };
    let c = channel(n);
    let data = data & 0xFF;
    for i in c.iface.iter_mut() {
        // (the registers of both drives take the write; LBA48 writes them twice: the older byte moves up)
        let shift = |v: i32| (v << 8 | data) & 0xFFFF;
        match reg {
            ATA_REG_FEATURES => i.features = shift(i.features),
            ATA_REG_SECTOR => i.sector_count = shift(i.sector_count),
            ATA_REG_LBA_LOW => i.lba_low = shift(i.lba_low),
            ATA_REG_LBA_MID => i.lba_mid = shift(i.lba_mid),
            ATA_REG_LBA_HIGH => i.lba_high = shift(i.lba_high),
            ATA_REG_DEVICE => {
                i.device = data;
                i.is_lba = data >> 6 & 1;
                i.head = data & 0xF;
            },
            _ => {},
        }
    }
    if reg == ATA_REG_DEVICE {
        c.current = (data & ATA_DR_DEV != 0) as i32;
    }
}
