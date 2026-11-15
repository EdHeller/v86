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
    Channel { iface: [IFACE0; 2], current: 0, device_control: 0, command_base: -1, control_base: -1, irq: 0, present: 0 };

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
