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

// ---------------------------------------------------------------------------------------------------------------
// The drives (the rest of src/ide.js): the commands (ATA and ATAPI), the data port (PIO, in single accesses and in
// blocks), the bus master DMA transfers and the identify data. The disk images are JavaScript's (their buffers load
// asynchronously): a read asks for bytes (js::ide_read; the answer comes to ide_io_buffer and ide_read_done), a write
// hands them over (js::ide_write; for DMA writes the end comes back as ide_write_done). Each such call is the last
// thing a function does: a buffer may answer at once, from inside the call.

use crate::cpu::cpu::{device_raise_irq, js};
use crate::cpu::global_pointers::memory_size;
use crate::cpu::memory::mem8;

const ATA_REG_DATA: i32 = 0;
const ATA_REG_COMMAND: i32 = 7;

const ATA_ER_ABRT: i32 = 0x04;
const ATA_SR_ERR: i32 = 0x01;
const ATA_SR_COND: i32 = 0x01;
const ATA_SR_DRQ: i32 = 0x08;
const ATA_SR_DSC: i32 = 0x10;
const ATA_SR_DF: i32 = 0x20;
const ATA_SR_DRDY: i32 = 0x40;
const ATA_SR_BSY: i32 = 0x80;
const ATA_CR_NIEN: i32 = 0x02;
const ATA_CR_SRST: i32 = 0x04;

const CMD_DEVICE_RESET: i32 = 0x08;
const CMD_EXECUTE_DEVICE_DIAGNOSTIC: i32 = 0x90;
const CMD_FLUSH_CACHE: i32 = 0xE7;
const CMD_FLUSH_CACHE_EXT: i32 = 0xEA;
const CMD_GET_MEDIA_STATUS: i32 = 0xDA;
const CMD_IDENTIFY_DEVICE: i32 = 0xEC;
const CMD_IDENTIFY_PACKET_DEVICE: i32 = 0xA1;
const CMD_IDLE_IMMEDIATE: i32 = 0xE1;
const CMD_INITIALIZE_DEVICE_PARAMETERS: i32 = 0x91;
const CMD_MEDIA_LOCK: i32 = 0xDE;
const CMD_NOP: i32 = 0x00;
const CMD_PACKET: i32 = 0xA0;
const CMD_READ_DMA: i32 = 0xC8;
const CMD_READ_DMA_EXT: i32 = 0x25;
const CMD_READ_MULTIPLE: i32 = 0x29;
const CMD_READ_MULTIPLE_EXT: i32 = 0xC4;
const CMD_READ_NATIVE_MAX_ADDRESS: i32 = 0xF8;
const CMD_READ_NATIVE_MAX_ADDRESS_EXT: i32 = 0x27;
const CMD_READ_SECTORS: i32 = 0x20;
const CMD_READ_SECTORS_EXT: i32 = 0x24;
const CMD_READ_VERIFY_SECTORS: i32 = 0x40;
const CMD_SECURITY_FREEZE_LOCK: i32 = 0xF5;
const CMD_SET_FEATURES: i32 = 0xEF;
const CMD_SET_MAX: i32 = 0xF9;
const CMD_SET_MULTIPLE_MODE: i32 = 0xC6;
const CMD_STANDBY_IMMEDIATE: i32 = 0xE0;
const CMD_WRITE_DMA: i32 = 0xCA;
const CMD_WRITE_DMA_EXT: i32 = 0x35;
const CMD_WRITE_MULTIPLE: i32 = 0x39;
const CMD_WRITE_MULTIPLE_EXT: i32 = 0xC5;
const CMD_WRITE_SECTORS: i32 = 0x30;
const CMD_WRITE_SECTORS_EXT: i32 = 0x34;
const CMD_READ_LOG_EXT: i32 = 0x2F;
const CMD_10H: i32 = 0x10;
const CMD_F0H: i32 = 0xF0;

const ATAPI_GET_CONFIGURATION: u8 = 0x46;
const ATAPI_GET_EVENT_STATUS_NOTIFICATION: u8 = 0x4A;
const ATAPI_INQUIRY: u8 = 0x12;
const ATAPI_MECHANISM_STATUS: u8 = 0xBD;
const ATAPI_MODE_SENSE_6: u8 = 0x1A;
const ATAPI_MODE_SENSE_10: u8 = 0x5A;
const ATAPI_PAUSE: u8 = 0x45;
const ATAPI_PREVENT_ALLOW_MEDIUM_REMOVAL: u8 = 0x1E;
const ATAPI_READ_10: u8 = 0x28;
const ATAPI_READ_12: u8 = 0xA8;
const ATAPI_READ_CAPACITY: u8 = 0x25;
const ATAPI_READ_FORMAT_CAPACITIES: u8 = 0x23;
const ATAPI_READ_CD: u8 = 0xBE;
const ATAPI_READ_DISK_INFORMATION: u8 = 0x51;
const ATAPI_READ_SUBCHANNEL: u8 = 0x42;
const ATAPI_READ_TOC_PMA_ATIP: u8 = 0x43;
const ATAPI_READ_TRACK_INFORMATION: u8 = 0x52;
const ATAPI_REQUEST_SENSE: u8 = 0x03;
const ATAPI_START_STOP_UNIT: u8 = 0x1B;
const ATAPI_TEST_UNIT_READY: u8 = 0x00;
const ATAPI_REPORT_KEY: u8 = 0xA4;
const ATAPI_READ_DISC_STRUCTURE: u8 = 0xAD;

const ATAPI_SIGNATURE_LO: i32 = 0x14;
const ATAPI_SIGNATURE_HI: i32 = 0xEB;
const SK_NOT_READY: i32 = 2;
const SK_ILLEGAL_REQUEST: i32 = 5;
const SK_UNIT_ATTENTION: i32 = 6;
const ASC_INV_FIELD_IN_CMD_PACKET: i32 = 0x24;
const ASC_MEDIUM_MAY_HAVE_CHANGED: i32 = 0x28;
const ASC_MEDIUM_NOT_PRESENT: i32 = 0x3A;
const MEDIUM_CHANGED_NONE: i32 = 0;
const MEDIUM_CHANGED_UNIT_ATTENTION: i32 = 1;
const MEDIUM_CHANGED_NOT_PRESENT: i32 = 2;

/// Whether a command (ATAPI) needs a disk
fn atapi_needs_disk(cmd: u8) -> bool {
    matches!(
        cmd,
        ATAPI_PAUSE
            | ATAPI_READ_10
            | ATAPI_READ_12
            | ATAPI_READ_CAPACITY
            | ATAPI_READ_FORMAT_CAPACITIES
            | ATAPI_READ_CD
            | ATAPI_READ_DISK_INFORMATION
            | ATAPI_READ_SUBCHANNEL
            | ATAPI_READ_TOC_PMA_ATIP
            | ATAPI_READ_TRACK_INFORMATION
            | ATAPI_TEST_UNIT_READY
            | ATAPI_READ_DISC_STRUCTURE
    )
}

/// A drive's fields that ide.js reads and writes (snapshots, the image): at these byte offsets
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DriveFields {
    is_atapi: i32,              // 0
    has_disk: i32,              // 4
    sector_size: i32,           // 8
    sectors_per_drq: i32,       // 12
    head_count: i32,            // 16
    sectors_per_track: i32,     // 20
    cylinder_count: i32,        // 24
    data_pointer: i32,          // 28
    data_length: i32,           // 32
    data_end: i32,              // 36
    current_command: i32,       // 40
    current_atapi_command: i32, // 44
    atapi_sense_key: i32,       // 48
    atapi_add_sense: i32,       // 52
    medium_changed: i32,        // 56
    core: i32,                  // 60: the drives are the core's (set by ide.js)
    sector_count: f64,          // 64
    write_dest: f64,            // 72
    disk_bytes: f64,            // 80
}

/// What a read or write that is under way goes on with when it ends
#[derive(Clone, Copy)]
enum Pending {
    AtaRead { cmd: i32, count: i32, byte_count: i32, is_single: bool },
    AtaReadDma { count: i32 },
    AtapiRead { req_length: i32 },
    AtapiReadDma,
    AtaWriteDma { count: i32 },
}

struct Drive {
    f: DriveFields,
    data: Vec<u8>,
    /// the bytes of a read (ide_io_buffer) or of a write
    io: Vec<u8>,
    pending: Option<(i32, Pending)>,
    next_id: i32,
}

static mut DRIVES: Option<Box<[[Drive; 2]; 2]>> = None;

#[allow(static_mut_refs)]
unsafe fn dv(n: usize, d: usize) -> &'static mut Drive {
    &mut DRIVES.get_or_insert_with(|| {
        let new = || Drive {
            f: DriveFields {
                is_atapi: 0,
                has_disk: 0,
                sector_size: 512,
                sectors_per_drq: 0x80,
                head_count: 0,
                sectors_per_track: 0,
                cylinder_count: 0,
                data_pointer: 0,
                data_length: 0,
                data_end: 0,
                current_command: -1,
                current_atapi_command: -1,
                atapi_sense_key: 0,
                atapi_add_sense: 0,
                medium_changed: 0,
                core: 0,
                sector_count: 0.0,
                write_dest: 0.0,
                disk_bytes: 0.0,
            },
            data: vec![0; 64 * 1024],
            io: Vec::new(),
            pending: None,
            next_id: 1,
        };
        Box::new([[new(), new()], [new(), new()]])
    })[n][d]
}

unsafe fn rg(n: usize, d: usize) -> &'static mut Iface { &mut channel(n).iface[d] }

/// The drives of a present channel are the core's
#[inline]
unsafe fn core_channel(n: usize) -> bool { channel(n).present != 0 && dv(n, 0).f.core != 0 }

unsafe fn push_irq(n: usize) {
    let c = channel(n);
    if c.device_control & ATA_CR_NIEN == 0 {
        c.dma_status |= 4;
        device_raise_irq(c.irq as u8);
    }
}

unsafe fn guest_read(addr: u32, len: usize) -> &'static [u8] {
    let size = *memory_size as usize;
    let start = (addr as usize).min(size);
    let end = (addr as usize + len).min(size);
    std::slice::from_raw_parts(mem8.add(start), end - start)
}

unsafe fn guest_write(addr: u32, bytes: &[u8]) {
    let size = *memory_size as usize;
    let start = (addr as usize).min(size);
    let end = (addr as usize + bytes.len()).min(size);
    if end > start {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), mem8.add(start), end - start);
        crate::jit::jit_dirty_cache(start as u32, end as u32);
    }
}

unsafe fn phys_read32(addr: u32) -> i32 {
    let b = guest_read(addr, 4);
    if b.len() < 4 { 0 } else { i32::from_le_bytes([b[0], b[1], b[2], b[3]]) }
}
unsafe fn phys_read16(addr: u32) -> i32 {
    let b = guest_read(addr, 2);
    if b.len() < 2 { 0 } else { u16::from_le_bytes([b[0], b[1]]) as i32 }
}
unsafe fn phys_read8(addr: u32) -> i32 { guest_read(addr, 1).first().copied().unwrap_or(0) as i32 }

impl Drive {
    fn data_allocate_noclear(&mut self, len: i32) {
        let len = len.max(0);
        if (self.data.len() as i32) < len {
            self.data = vec![0; (len as usize + 3) & !3];
        }
        self.f.data_length = len;
        self.f.data_pointer = 0;
    }
    fn data_allocate(&mut self, len: i32) {
        self.data_allocate_noclear(len);
        let end = ((len.max(0) as usize + 3) & !3).min(self.data.len());
        self.data[..end].fill(0);
    }
    fn data_set(&mut self, bytes: &[u8]) {
        self.data_allocate_noclear(bytes.len() as i32);
        self.data[..bytes.len()].copy_from_slice(bytes);
    }
    /// (the bytes of a finished read, as data_set)
    fn data_set_io(&mut self) {
        let io = std::mem::take(&mut self.io);
        self.data_set(&io);
        self.io = io;
    }
    fn put(&mut self, at: usize, bytes: &[u8]) {
        if self.data.len() < at + bytes.len() {
            self.data.resize(at + bytes.len(), 0);
        }
        self.data[at..at + bytes.len()].copy_from_slice(bytes);
    }
    fn has_disk(&self) -> bool { self.f.has_disk != 0 }
}

unsafe fn cancel_io(n: usize, d: usize) { dv(n, d).pending = None; }

unsafe fn device_reset(n: usize, d: usize) {
    let r = rg(n, d);
    if dv(n, d).f.is_atapi != 0 {
        r.status = 0;
        r.sector_count = 1;
        r.error = 1;
        r.lba_low = 1;
        r.lba_mid = ATAPI_SIGNATURE_LO;
        r.lba_high = ATAPI_SIGNATURE_HI;
    }
    else {
        r.status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_ERR;
        r.sector_count = 1;
        r.error = 1;
        r.lba_low = 1;
        r.lba_mid = 0;
        r.lba_high = 0;
    }
    cancel_io(n, d);
}

unsafe fn ata_abort_command(n: usize, d: usize) {
    let r = rg(n, d);
    r.error = ATA_ER_ABRT;
    r.status = ATA_SR_DRDY | ATA_SR_ERR;
    push_irq(n);
}

unsafe fn get_chs(n: usize, d: usize) -> f64 {
    let r = rg(n, d);
    let f = &dv(n, d).f;
    let c = (r.lba_mid & 0xFF | r.lba_high << 8 & 0xFF00) as f64;
    ((c * f.head_count as f64 + r.head as f64) * f.sectors_per_track as f64) + (r.lba_low & 0xFF) as f64 - 1.0
}

unsafe fn get_lba28(n: usize, d: usize) -> f64 {
    let r = rg(n, d);
    (r.lba_low & 0xFF | r.lba_mid << 8 & 0xFF00 | r.lba_high << 16 & 0xFF0000 | (r.head & 0xF) << 24) as f64
}

unsafe fn get_lba48(n: usize, d: usize) -> f64 {
    let r = rg(n, d);
    ((r.lba_low & 0xFF | r.lba_mid << 8 & 0xFF00 | r.lba_high << 16 & 0xFF0000) as u32 | ((r.lba_low >> 8) << 24) as u32 & 0xFF000000)
        as f64
}

unsafe fn get_lba(n: usize, d: usize, is_lba48: bool) -> f64 {
    if is_lba48 {
        get_lba48(n, d)
    }
    else if rg(n, d).is_lba != 0 {
        get_lba28(n, d)
    }
    else {
        get_chs(n, d)
    }
}

unsafe fn get_count(n: usize, d: usize, is_lba48: bool) -> i32 {
    let r = rg(n, d);
    if is_lba48 {
        if r.sector_count == 0 { 0x10000 } else { r.sector_count }
    }
    else {
        let c = r.sector_count & 0xFF;
        if c == 0 { 0x100 } else { c }
    }
}

unsafe fn ata_advance(n: usize, d: usize, cmd: i32, sectors: i32) {
    let r = rg(n, d);
    r.sector_count -= sectors;
    if matches!(
        cmd,
        CMD_READ_SECTORS_EXT | CMD_READ_MULTIPLE | CMD_READ_DMA_EXT | CMD_WRITE_SECTORS_EXT | CMD_WRITE_MULTIPLE | CMD_WRITE_DMA_EXT
    ) {
        let s = (sectors as f64 + get_lba48(n, d)) as i64 as i32;
        let r = rg(n, d);
        r.lba_low = s & 0xFF | s >> 16 & 0xFF00;
        r.lba_mid = s >> 8 & 0xFF;
        r.lba_high = s >> 16 & 0xFF;
    }
    else if r.is_lba != 0 {
        let s = (sectors as f64 + get_lba28(n, d)) as i64 as i32;
        let r = rg(n, d);
        r.lba_low = s & 0xFF;
        r.lba_mid = s >> 8 & 0xFF;
        r.lba_high = s >> 16 & 0xFF;
        r.head = r.head & !0xF | s & 0xF;
    }
    else {
        let s = sectors as f64 + get_chs(n, d);
        let f = dv(n, d).f;
        let r = rg(n, d);
        let c = (s / (f.head_count * f.sectors_per_track) as f64) as i64 as i32;
        r.lba_mid = c & 0xFF;
        r.lba_high = c >> 8 & 0xFF;
        r.head = ((s / f.sectors_per_track as f64) as i64 as i32 % f.head_count.max(1)) & 0xF;
        r.lba_low = ((s % f.sectors_per_track as f64) as i64 as i32 + 1) & 0xFF;
    }
}

/// Ask JavaScript for `length` bytes of the image at `start`; `pending` goes on when they come (ide_read_done)
unsafe fn read_image(n: usize, d: usize, start: f64, length: i32, pending: Pending) {
    let drive = dv(n, d);
    let id = drive.next_id;
    drive.next_id = drive.next_id.wrapping_add(1).max(1);
    drive.pending = Some((id, pending));
    js::ide_read(n as i32, d as i32, id, start, length as f64);
}

unsafe fn ata_command(n: usize, d: usize, cmd: i32) {
    if rg(n, d).drive_connected == 0 && cmd != CMD_EXECUTE_DEVICE_DIAGNOSTIC {
        return;
    }
    let atapi = dv(n, d).f.is_atapi != 0;
    dv(n, d).f.current_command = cmd;
    rg(n, d).error = 0;
    let ok = |n: usize, d: usize| {
        rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC;
        push_irq(n);
    };
    match cmd {
        CMD_DEVICE_RESET => {
            let drive = dv(n, d);
            drive.f.data_pointer = 0;
            drive.f.data_end = 0;
            drive.f.data_length = 0;
            device_reset(n, d);
            push_irq(n);
        },
        CMD_10H => {
            rg(n, d).lba_mid = 0;
            ok(n, d);
        },
        CMD_READ_NATIVE_MAX_ADDRESS | CMD_READ_NATIVE_MAX_ADDRESS_EXT => {
            let last = (dv(n, d).f.sector_count - 1.0) as i64 as i32;
            let r = rg(n, d);
            r.lba_low = last & 0xFF;
            r.lba_mid = last >> 8 & 0xFF;
            r.lba_high = last >> 16 & 0xFF;
            if cmd == CMD_READ_NATIVE_MAX_ADDRESS {
                r.device = r.device & 0xF0 | last >> 24 & 0x0F;
            }
            else {
                r.lba_low |= last >> 24 << 8 & 0xFF00;
            }
            ok(n, d);
        },
        CMD_READ_SECTORS | CMD_READ_SECTORS_EXT | CMD_READ_MULTIPLE | CMD_READ_MULTIPLE_EXT => {
            if atapi {
                if cmd == CMD_READ_SECTORS {
                    rg(n, d).lba_mid = ATAPI_SIGNATURE_LO;
                    rg(n, d).lba_high = ATAPI_SIGNATURE_HI;
                }
                ata_abort_command(n, d);
            }
            else {
                ata_read_sectors(n, d, cmd);
            }
        },
        CMD_WRITE_SECTORS | CMD_WRITE_SECTORS_EXT | CMD_WRITE_MULTIPLE | CMD_WRITE_MULTIPLE_EXT => {
            if atapi { ata_abort_command(n, d) } else { ata_write_sectors(n, d, cmd) }
        },
        CMD_EXECUTE_DEVICE_DIAGNOSTIC => {
            for drive in 0..2 {
                if drive != 0 && rg(n, drive).drive_connected == 0 {
                    continue;
                }
                let is_atapi = dv(n, drive).f.is_atapi != 0;
                let r = rg(n, drive);
                r.sector_count = 1;
                r.lba_low = 1;
                r.lba_mid = if is_atapi { ATAPI_SIGNATURE_LO } else { 0 };
                r.lba_high = if is_atapi { ATAPI_SIGNATURE_HI } else { 0 };
                r.status = if is_atapi { 0 } else { ATA_SR_DRDY | ATA_SR_DSC };
                r.error = 0x01;
            }
            push_irq(n);
        },
        CMD_INITIALIZE_DEVICE_PARAMETERS | CMD_READ_VERIFY_SECTORS | CMD_STANDBY_IMMEDIATE | CMD_IDLE_IMMEDIATE
        | CMD_FLUSH_CACHE | CMD_FLUSH_CACHE_EXT | CMD_SET_FEATURES | CMD_SECURITY_FREEZE_LOCK => ok(n, d),
        CMD_PACKET => {
            if atapi {
                let drive = dv(n, d);
                drive.data_allocate(12);
                drive.f.data_end = 12;
                rg(n, d).sector_count = 0x01;
                rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
            }
            else {
                ata_abort_command(n, d);
            }
        },
        CMD_IDENTIFY_PACKET_DEVICE => {
            if atapi {
                create_identify_packet(n, d);
                rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
                push_irq(n);
            }
            else {
                ata_abort_command(n, d);
            }
        },
        CMD_SET_MULTIPLE_MODE => {
            dv(n, d).f.sectors_per_drq = rg(n, d).sector_count & 0xFF;
            ok(n, d);
        },
        CMD_READ_DMA | CMD_READ_DMA_EXT => ata_read_sectors_dma(n, d, cmd),
        CMD_WRITE_DMA | CMD_WRITE_DMA_EXT => ata_write_sectors_dma(n, d, cmd),
        CMD_GET_MEDIA_STATUS => {
            if atapi {
                let drive = dv(n, d);
                let r = rg(n, d);
                if !drive.has_disk() {
                    r.error |= 0x02;
                }
                if drive.f.medium_changed != 0 {
                    r.error |= 0x20;
                    drive.f.medium_changed = MEDIUM_CHANGED_NONE;
                }
                r.error |= 0x40;
            }
            ok(n, d);
        },
        CMD_IDENTIFY_DEVICE => {
            if atapi {
                rg(n, d).lba_mid = ATAPI_SIGNATURE_LO;
                rg(n, d).lba_high = ATAPI_SIGNATURE_HI;
                ata_abort_command(n, d);
            }
            else {
                create_identify_packet(n, d);
                rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
                push_irq(n);
            }
        },
        CMD_MEDIA_LOCK => {
            rg(n, d).status = ATA_SR_DRDY;
            push_irq(n);
        },
        CMD_READ_LOG_EXT | CMD_SET_MAX | CMD_NOP | CMD_F0H => ata_abort_command(n, d),
        _ => ata_abort_command(n, d),
    }
}

unsafe fn create_identify_packet(n: usize, d: usize) {
    let drive = dv(n, d);
    let f = drive.f;
    let atapi = f.is_atapi != 0;
    let cylinders = f.cylinder_count.min(16383);
    let general_cfg = if atapi { 0x8540 } else { 0x0040 };
    let multiword_dma_mode = if f.current_command == CMD_PACKET { 0 } else { 0x0407 };
    let feat_82 = if atapi { 1 << 14 | 1 << 9 | 1 << 5 } else { 1 << 14 };
    let feat_83 = if atapi { 1 << 14 | 1 << 12 } else { 1 << 14 | 1 << 13 | 1 << 12 | 1 << 10 };
    let feat_84 = 1 << 14;
    let sc = f.sector_count as i64 as i32;
    let spt = f.sectors_per_track;
    let mut w = [0i32; 102];
    w[0] = general_cfg;
    w[1] = cylinders;
    w[3] = f.head_count;
    w[4] = spt / 512;
    w[5] = 512;
    w[6] = spt;
    w[20] = 3;
    w[21] = 0x200;
    w[22] = 4;
    w[47] = 0x80;
    w[48] = 1;
    w[49] = 0x200;
    w[51] = 0x200;
    w[52] = 0x200;
    w[53] = 7;
    w[54] = cylinders;
    w[55] = f.head_count;
    w[56] = spt & 0xFF;
    w[57] = sc & 0xFFFF;
    w[58] = sc >> 16 & 0xFFFF;
    w[60] = sc & 0xFFFF;
    w[61] = sc >> 16 & 0xFFFF;
    w[63] = multiword_dma_mode;
    w[65..69].fill(30);
    w[82] = feat_82;
    w[83] = feat_83;
    w[84] = feat_84;
    w[85] = feat_82;
    w[86] = feat_83;
    w[87] = feat_84;
    w[93] = 0x6001;
    w[100] = sc & 0xFFFF;
    w[101] = sc >> 16 & 0xFFFF;
    let mut bytes = [0u8; 204];
    for (i, v) in w.iter().enumerate() {
        bytes[2 * i] = *v as u8;
        bytes[2 * i + 1] = (*v >> 8) as u8;
    }
    drive.data[..512].fill(0);
    drive.data_set(&bytes);
    // the strings: big-endian words (serial number, firmware revision, model)
    let strcpy_be16 = |data: &mut Vec<u8>, ofs16: usize, len16: usize, s: &[u8]| {
        let mut ofs8 = ofs16 << 1;
        let end8 = ofs8 + (len16 << 1);
        for (i, &c) in s.iter().enumerate() {
            if ofs8 >= end8 {
                break;
            }
            if i & 1 != 0 {
                data[ofs8] = c;
                ofs8 += 2;
            }
            else {
                data[ofs8 + 1] = c;
            }
        }
    };
    let serial = format!("8086-86{}{}", n, d);
    strcpy_be16(&mut drive.data, 10, 10, serial.as_bytes());
    strcpy_be16(&mut drive.data, 23, 4, b"1.00");
    strcpy_be16(&mut drive.data, 27, 20, if atapi { b"v86 ATAPI CD-ROM" } else { b"v86 ATA HD" });
    drive.f.data_length = 512;
    drive.f.data_end = 512;
}

unsafe fn ata_read_sectors(n: usize, d: usize, cmd: i32) {
    let is_lba48 = cmd == CMD_READ_SECTORS_EXT || cmd == CMD_READ_MULTIPLE;
    let count = get_count(n, d, is_lba48);
    let lba = get_lba(n, d, is_lba48);
    let is_single = cmd == CMD_READ_SECTORS || cmd == CMD_READ_SECTORS_EXT;
    let f = dv(n, d).f;
    let byte_count = count * f.sector_size;
    let start = lba * f.sector_size as f64;
    if start + byte_count as f64 > f.disk_bytes {
        rg(n, d).status = 0xFF;
        push_irq(n);
    }
    else {
        rg(n, d).status = ATA_SR_DRDY | ATA_SR_BSY;
        read_image(n, d, start, byte_count, Pending::AtaRead { cmd, count, byte_count, is_single });
    }
}

unsafe fn ata_read_sectors_dma(n: usize, d: usize, cmd: i32) {
    let is_lba48 = cmd == CMD_READ_DMA_EXT;
    let count = get_count(n, d, is_lba48);
    let lba = get_lba(n, d, is_lba48);
    let f = dv(n, d).f;
    if lba * f.sector_size as f64 + (count * f.sector_size) as f64 > f.disk_bytes {
        rg(n, d).status = 0xFF;
        push_irq(n);
        return;
    }
    rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
    channel(n).dma_status |= 1;
}

unsafe fn do_ata_read_sectors_dma(n: usize, d: usize) {
    let cmd = dv(n, d).f.current_command;
    let is_lba48 = cmd == CMD_READ_DMA_EXT;
    let count = get_count(n, d, is_lba48);
    let lba = get_lba(n, d, is_lba48);
    let f = dv(n, d).f;
    let byte_count = count * f.sector_size;
    read_image(n, d, lba * f.sector_size as f64, byte_count, Pending::AtaReadDma { count });
}

unsafe fn ata_write_sectors(n: usize, d: usize, cmd: i32) {
    let is_lba48 = cmd == CMD_WRITE_SECTORS_EXT || cmd == CMD_WRITE_MULTIPLE;
    let count = get_count(n, d, is_lba48);
    let lba = get_lba(n, d, is_lba48);
    let is_single = cmd == CMD_WRITE_SECTORS || cmd == CMD_WRITE_SECTORS_EXT;
    let drive = dv(n, d);
    let byte_count = count * drive.f.sector_size;
    let start = lba * drive.f.sector_size as f64;
    if start + byte_count as f64 > drive.f.disk_bytes {
        rg(n, d).status = 0xFF;
        push_irq(n);
    }
    else {
        rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
        drive.data_allocate_noclear(byte_count);
        drive.f.data_end = if is_single { 512 } else { byte_count.min(drive.f.sectors_per_drq * 512) };
        drive.f.write_dest = start;
    }
}

unsafe fn ata_write_sectors_dma(n: usize, d: usize, cmd: i32) {
    let is_lba48 = cmd == CMD_WRITE_DMA_EXT;
    let count = get_count(n, d, is_lba48);
    let lba = get_lba(n, d, is_lba48);
    let f = dv(n, d).f;
    if lba * f.sector_size as f64 + (count * f.sector_size) as f64 > f.disk_bytes {
        rg(n, d).status = 0xFF;
        push_irq(n);
        return;
    }
    rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
    channel(n).dma_status |= 1;
}

unsafe fn do_ata_write_sectors_dma(n: usize, d: usize) {
    let cmd = dv(n, d).f.current_command;
    let is_lba48 = cmd == CMD_WRITE_DMA_EXT;
    let count = get_count(n, d, is_lba48);
    let lba = get_lba(n, d, is_lba48);
    let drive = dv(n, d);
    let byte_count = (count * drive.f.sector_size) as usize;
    let start = lba * drive.f.sector_size as f64;
    let mut prdt = channel(n).prdt_addr as u32;
    let mut buffer = vec![0u8; byte_count];
    let mut offset = 0usize;
    loop {
        let addr = phys_read32(prdt) as u32;
        let mut prd_count = phys_read16(prdt + 4) as usize;
        let end = phys_read8(prdt + 7) & 0x80 != 0;
        if prd_count == 0 {
            prd_count = 0x10000;
        }
        let slice = guest_read(addr, prd_count);
        let take = slice.len().min(buffer.len().saturating_sub(offset));
        buffer[offset..offset + take].copy_from_slice(&slice[..take]);
        offset += prd_count;
        prdt = prdt.wrapping_add(8);
        if end || offset > (1 << 28) {
            break;
        }
    }
    let id = drive.next_id;
    drive.next_id = drive.next_id.wrapping_add(1).max(1);
    drive.pending = Some((id, Pending::AtaWriteDma { count }));
    drive.io = buffer;
    js::ide_write(n as i32, d as i32, id, start, drive.io.as_ptr() as u32, byte_count as i32, true);
}

/// PIO: the sectors written: to the image (nothing comes back)
unsafe fn do_write(n: usize, d: usize) {
    rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC;
    let drive = dv(n, d);
    let length = drive.f.data_length;
    let cmd = drive.f.current_command;
    ata_advance(n, d, cmd, length / 512);
    push_irq(n);
    let drive = dv(n, d);
    js::ide_write(n as i32, d as i32, 0, drive.f.write_dest, drive.data.as_ptr() as u32, length, false);
}

unsafe fn atapi_check_condition_response(n: usize, d: usize, sense_key: i32, additional_sense: i32) {
    let drive = dv(n, d);
    drive.data_allocate(0);
    drive.f.data_end = drive.f.data_length;
    let r = rg(n, d);
    r.status = ATA_SR_DRDY | ATA_SR_COND;
    r.error = sense_key << 4;
    r.sector_count = r.sector_count & !7 | 2 | 1;
    drive.f.atapi_sense_key = sense_key;
    drive.f.atapi_add_sense = additional_sense;
}

unsafe fn atapi_handle(n: usize, d: usize) {
    let drive = dv(n, d);
    let cmd = drive.data[0];
    drive.f.data_pointer = 0;
    drive.f.current_atapi_command = cmd as i32;

    if drive.f.medium_changed != 0 && drive.has_disk() && cmd != ATAPI_REQUEST_SENSE && cmd != ATAPI_INQUIRY {
        if drive.f.medium_changed == MEDIUM_CHANGED_NOT_PRESENT {
            drive.f.medium_changed = MEDIUM_CHANGED_UNIT_ATTENTION;
            atapi_check_condition_response(n, d, SK_NOT_READY, ASC_MEDIUM_NOT_PRESENT);
        }
        else {
            drive.f.medium_changed = MEDIUM_CHANGED_NONE;
            atapi_check_condition_response(n, d, SK_UNIT_ATTENTION, ASC_MEDIUM_MAY_HAVE_CHANGED);
        }
        push_irq(n);
        return;
    }
    if cmd != ATAPI_REQUEST_SENSE {
        drive.f.atapi_sense_key = 0;
        drive.f.atapi_add_sense = 0;
    }
    if !drive.has_disk() && atapi_needs_disk(cmd) {
        atapi_check_condition_response(n, d, SK_NOT_READY, ASC_MEDIUM_NOT_PRESENT);
        push_irq(n);
        return;
    }

    let ready = ATA_SR_DRDY | ATA_SR_DSC;
    let with_data = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
    let b = |i: usize| dv(n, d).data[i] as i32;
    let sector_count = drive.f.sector_count;
    let sector_size = drive.f.sector_size;
    // (the commands' replies)
    let no_data = |n: usize, d: usize, status: i32| {
        let drive = dv(n, d);
        drive.data_allocate(0);
        drive.f.data_end = drive.f.data_length;
        rg(n, d).status = status;
    };
    match cmd {
        ATAPI_TEST_UNIT_READY => {
            if drive.has_disk() {
                no_data(n, d, ready);
            }
            else {
                atapi_check_condition_response(n, d, SK_NOT_READY, ASC_MEDIUM_NOT_PRESENT);
            }
        },
        ATAPI_REQUEST_SENSE => {
            let len = b(4);
            drive.data_allocate(len);
            drive.f.data_end = drive.f.data_length;
            rg(n, d).status = with_data;
            let (key, add) = (drive.f.atapi_sense_key, drive.f.atapi_add_sense);
            drive.put(0, &[0x80 | 0x70]);
            drive.put(2, &[key as u8]);
            drive.put(7, &[8]);
            drive.put(12, &[add as u8]);
            drive.f.atapi_sense_key = 0;
            drive.f.atapi_add_sense = 0;
        },
        ATAPI_INQUIRY => {
            let length = b(4);
            rg(n, d).status = with_data;
            drive.put(0, &[
                0x05, 0x80, 0x01, 0x31, 31, 0, 0, 0, 0x53, 0x4F, 0x4E, 0x59, 0x20, 0x20, 0x20, 0x20, 0x43, 0x44, 0x2D, 0x52,
                0x4F, 0x4D, 0x20, 0x43, 0x44, 0x55, 0x2D, 0x31, 0x30, 0x30, 0x30, 0x20, 0x31, 0x2E, 0x31, 0x61,
            ]);
            drive.f.data_length = length.min(36);
            drive.f.data_end = drive.f.data_length;
        },
        ATAPI_MODE_SENSE_6 => {
            drive.data_allocate(b(4));
            drive.f.data_end = drive.f.data_length;
            rg(n, d).status = with_data;
        },
        ATAPI_PREVENT_ALLOW_MEDIUM_REMOVAL | ATAPI_READ_DISK_INFORMATION | 0xBB | ATAPI_READ_CD => no_data(n, d, ready),
        ATAPI_READ_CAPACITY => {
            let count = (sector_count - 1.0) as i64 as i32;
            drive.data_set(&[
                (count >> 24) as u8,
                (count >> 16) as u8,
                (count >> 8) as u8,
                count as u8,
                0,
                0,
                (sector_size >> 8) as u8,
                sector_size as u8,
            ]);
            drive.f.data_end = drive.f.data_length;
            rg(n, d).status = with_data;
        },
        ATAPI_READ_FORMAT_CAPACITIES => {
            let count = (sector_count - 1.0) as i64 as i32;
            drive.data_set(&[
                0,
                0,
                0,
                8,
                (count >> 24) as u8,
                (count >> 16) as u8,
                (count >> 8) as u8,
                count as u8,
                2,
                (sector_size >> 16) as u8,
                (sector_size >> 8) as u8,
                sector_size as u8,
            ]);
            drive.f.data_end = drive.f.data_length;
            rg(n, d).status = with_data;
        },
        ATAPI_READ_10 | ATAPI_READ_12 => {
            let mut packet = [0u8; 12];
            packet.copy_from_slice(&drive.data[..12]);
            if rg(n, d).features & 1 != 0 {
                atapi_read_dma(n, d, &packet);
            }
            else {
                atapi_read(n, d, &packet);
            }
        },
        ATAPI_READ_SUBCHANNEL => {
            drive.data_allocate(b(8).min(8));
            drive.f.data_end = drive.f.data_length;
            rg(n, d).status = with_data;
        },
        ATAPI_READ_TOC_PMA_ATIP => {
            let length = b(8) | b(7) << 8;
            let format = b(9) >> 6;
            let toc_msf = b(1) & 2 != 0;
            drive.data_allocate(length);
            drive.f.data_end = drive.f.data_length;
            if format == 0 {
                let addr = |lba: i64| -> [u8; 4] {
                    if toc_msf {
                        [0, ((lba + 150) / (75 * 60)) as u8, (((lba + 150) / 75) % 60) as u8, ((lba + 150) % 75) as u8]
                    }
                    else {
                        let l = lba as i32;
                        [(l >> 24) as u8, (l >> 16) as u8, (l >> 8) as u8, l as u8]
                    }
                };
                let start = addr(0);
                let leadout = addr(sector_count as i64);
                drive.put(0, &[
                    0, 18, 1, 1, 0, 0x14, 1, 0, start[0], start[1], start[2], start[3], 0, 0x16, 0xAA, 0, leadout[0],
                    leadout[1], leadout[2], leadout[3],
                ]);
            }
            else if format == 1 {
                drive.put(0, &[0, 10, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
            }
            rg(n, d).status = with_data;
        },
        ATAPI_GET_CONFIGURATION => {
            let length = (b(8) | b(7) << 8).min(32);
            drive.data_allocate(length);
            drive.f.data_end = drive.f.data_length;
            let l = length - 4;
            drive.put(0, &[(l >> 24) as u8, (l >> 16) as u8, (l >> 8) as u8, l as u8]);
            drive.put(6, &[0x08]);
            drive.put(10, &[3]);
            rg(n, d).status = with_data;
        },
        ATAPI_READ_TRACK_INFORMATION | ATAPI_PAUSE | ATAPI_GET_EVENT_STATUS_NOTIFICATION | ATAPI_REPORT_KEY
        | ATAPI_READ_DISC_STRUCTURE => {
            atapi_check_condition_response(n, d, SK_ILLEGAL_REQUEST, ASC_INV_FIELD_IN_CMD_PACKET);
        },
        ATAPI_MODE_SENSE_10 => {
            let length = b(8) | b(7) << 8;
            if b(2) == 0x2A {
                drive.data_allocate(length.min(30));
            }
            drive.f.data_end = drive.f.data_length;
            rg(n, d).status = with_data;
        },
        ATAPI_MECHANISM_STATUS => {
            drive.data_allocate(b(9) | b(8) << 8);
            drive.f.data_end = drive.f.data_length;
            drive.put(5, &[1]);
            rg(n, d).status = with_data;
        },
        ATAPI_START_STOP_UNIT => {
            let loej_start = b(4) & 3;
            if drive.has_disk() && loej_start == 2 {
                drive.f.medium_changed = MEDIUM_CHANGED_NOT_PRESENT;
                drive.f.has_disk = 0;
                // (the image goes: JavaScript forgets it)
                js::ide_event(n as i32, d as i32, 0);
            }
            no_data(n, d, ready);
        },
        _ => atapi_check_condition_response(n, d, SK_ILLEGAL_REQUEST, ASC_INV_FIELD_IN_CMD_PACKET),
    }
    atapi_finish(n, d);
}

/// After an ATAPI command: the interrupt reason, the interrupt, and without data the end of the command
unsafe fn atapi_finish(n: usize, d: usize) {
    let r = rg(n, d);
    r.sector_count = r.sector_count & !7 | 2;
    if r.status & ATA_SR_BSY == 0 {
        push_irq(n);
    }
    let r = rg(n, d);
    if r.status & ATA_SR_BSY == 0 && dv(n, d).f.data_length == 0 {
        r.sector_count |= 1;
        r.status &= !ATA_SR_DRQ;
    }
}

fn atapi_lba_count(cmd: &[u8; 12]) -> (i64, i64) {
    let lba = (cmd[2] as i32) << 24 | (cmd[3] as i32) << 16 | (cmd[4] as i32) << 8 | cmd[5] as i32;
    let count = if cmd[0] == ATAPI_READ_12 {
        ((cmd[6] as u32) << 24 | (cmd[7] as u32) << 16 | (cmd[8] as u32) << 8 | cmd[9] as u32) as i64
    }
    else {
        ((cmd[7] as i64) << 8 | cmd[8] as i64) as i64
    };
    (lba as i64, count)
}

unsafe fn atapi_read(n: usize, d: usize, cmd: &[u8; 12]) {
    let (lba, count) = atapi_lba_count(cmd);
    let drive = dv(n, d);
    let mut byte_count = (count * drive.f.sector_size as i64) as f64;
    let start = (lba * drive.f.sector_size as i64) as f64;
    drive.f.data_length = 0;
    let r = rg(n, d);
    let mut req_length = (r.lba_high << 8 & 0xFF00 | r.lba_mid & 0xFF) as f64;
    r.lba_mid = 0;
    r.lba_high = 0;
    if req_length == 65535.0 {
        req_length -= 1.0;
    }
    if req_length > byte_count {
        req_length = byte_count;
    }
    if !drive.has_disk() {
        r.status = 0xFF;
        r.error = 0x41;
        push_irq(n);
    }
    else if start >= drive.f.disk_bytes {
        r.status = 0xFF;
        push_irq(n);
    }
    else if byte_count == 0.0 {
        r.status = ATA_SR_DRDY | ATA_SR_DSC;
        drive.f.data_pointer = 0;
    }
    else {
        byte_count = byte_count.min(drive.f.disk_bytes - start);
        r.status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_BSY;
        read_image(n, d, start, byte_count as i32, Pending::AtapiRead { req_length: req_length as i32 });
    }
}

unsafe fn atapi_read_dma(n: usize, d: usize, cmd: &[u8; 12]) {
    let (lba, count) = atapi_lba_count(cmd);
    let drive = dv(n, d);
    let byte_count = (count * drive.f.sector_size as i64) as f64;
    let start = (lba * drive.f.sector_size as i64) as f64;
    if start >= drive.f.disk_bytes {
        rg(n, d).status = 0xFF;
        push_irq(n);
    }
    else {
        rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_BSY;
        read_image(n, d, start, byte_count as i32, Pending::AtapiReadDma);
    }
}

unsafe fn do_atapi_dma(n: usize, d: usize) {
    if channel(n).dma_status & 1 == 0 || rg(n, d).status & ATA_SR_DRQ == 0 {
        return;
    }
    let drive = dv(n, d);
    let mut prdt = channel(n).prdt_addr as u32;
    let mut offset = 0usize;
    let length = drive.f.data_length.max(0) as usize;
    loop {
        let addr = phys_read32(prdt) as u32;
        let mut count = phys_read16(prdt + 4) as usize;
        let end = phys_read8(prdt + 7) & 0x80 != 0;
        if count == 0 {
            count = 0x10000;
        }
        let from = offset.min(length);
        let to = (offset + count).min(length);
        guest_write(addr, &drive.data[from..to]);
        offset += count;
        prdt = prdt.wrapping_add(8);
        if offset >= length && !end {
            break;
        }
        if end {
            break;
        }
    }
    rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC;
    channel(n).dma_status &= !1;
    let r = rg(n, d);
    r.sector_count = r.sector_count & !7 | 3;
    push_irq(n);
}

unsafe fn read_end(n: usize, d: usize) {
    let drive = dv(n, d);
    let r = rg(n, d);
    if drive.f.current_command == CMD_PACKET {
        if drive.f.data_end == drive.f.data_length {
            r.status = ATA_SR_DRDY | ATA_SR_DSC;
            r.sector_count = r.sector_count & !7 | 3;
            push_irq(n);
        }
        else {
            r.status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
            r.sector_count = r.sector_count & !7 | 2;
            push_irq(n);
            let r = rg(n, d);
            let byte_count = r.lba_high << 8 & 0xFF00 | r.lba_mid & 0xFF;
            if drive.f.data_end + byte_count > drive.f.data_length {
                let left = drive.f.data_length - drive.f.data_end;
                r.lba_mid = left & 0xFF;
                r.lba_high = left >> 8 & 0xFF;
                drive.f.data_end = drive.f.data_length;
            }
            else {
                drive.f.data_end += byte_count;
            }
        }
    }
    else {
        r.error = 0;
        if drive.f.data_pointer >= drive.f.data_length {
            r.status = ATA_SR_DRDY | ATA_SR_DSC;
        }
        else {
            let cmd = drive.f.current_command;
            let sector_count = if cmd == CMD_READ_MULTIPLE || cmd == CMD_READ_MULTIPLE_EXT {
                drive.f.sectors_per_drq.min((drive.f.data_length - drive.f.data_end) / 512)
            }
            else {
                1
            };
            ata_advance(n, d, cmd, sector_count);
            let drive = dv(n, d);
            drive.f.data_end += 512 * sector_count;
            rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
            push_irq(n);
        }
    }
}

unsafe fn write_end(n: usize, d: usize) {
    let drive = dv(n, d);
    if drive.f.current_command == CMD_PACKET {
        atapi_handle(n, d);
    }
    else if drive.f.data_pointer >= drive.f.data_length {
        do_write(n, d);
    }
    else {
        rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
        drive.f.data_end += 512;
        push_irq(n);
    }
}

unsafe fn read_data(n: usize, d: usize, length: i32) -> i32 {
    let drive = dv(n, d);
    let dp = drive.f.data_pointer;
    if dp < drive.f.data_end {
        let at = dp as usize;
        let data = &drive.data;
        let get = |i: usize| data.get(i).copied().unwrap_or(0) as i32;
        let result = match length {
            1 => get(at),
            2 => {
                let a = at & !1;
                get(a) | get(a + 1) << 8
            },
            _ => {
                let a = at & !3;
                get(a) | get(a + 1) << 8 | get(a + 2) << 16 | get(a + 3) << 24
            },
        };
        drive.f.data_pointer += length;
        if drive.f.data_pointer >= drive.f.data_end {
            read_end(n, d);
        }
        result
    }
    else {
        drive.f.data_pointer += length;
        0
    }
}

unsafe fn write_data_port(n: usize, d: usize, data: i32, length: i32) {
    let drive = dv(n, d);
    if drive.f.data_pointer >= drive.f.data_end {
        return;
    }
    let at = drive.f.data_pointer as usize;
    match length {
        1 => drive.put(at, &[data as u8]),
        2 => drive.put(at & !1, &(data as u16).to_le_bytes()),
        _ => drive.put(at & !3, &data.to_le_bytes()),
    }
    drive.f.data_pointer += length;
    if drive.f.data_pointer == drive.f.data_end {
        write_end(n, d);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the ports, when the drives are the core's

/// The current drive of a channel
unsafe fn cur(n: usize) -> usize { channel(n).current as usize & 1 }

/// (channel, register 0..7 of the command block or 8 for the control block)
#[inline]
unsafe fn decode_core(port: i32) -> Option<(usize, i32)> { decode(port).filter(|&(n, _)| core_channel(n)) }

/// Whether the core answers this access (the drives are the core's): the sizes ide.js registered
#[inline]
pub unsafe fn core_serves(port: i32, size: i32) -> bool {
    if super::parallel::WORKER {
        return false;
    }
    if (0x170..=0x177).contains(&port) || (0x1F0..=0x1F7).contains(&port) || port == 0x376 || port == 0x3F6 {
        return match decode_core(port) {
            Some((_, ATA_REG_DATA)) => true,
            Some(_) => size == 8,
            None => false,
        };
    }
    match bus_master(port) {
        Some((n, reg)) if core_channel(n) => matches!((reg, size), (0, 8) | (0, 32) | (2, 8) | (4, 32)),
        _ => false,
    }
}

#[no_mangle]
pub unsafe fn ide_core_read(port: i32, size: i32) -> i32 {
    if let Some((n, reg)) = decode_core(port) {
        return if reg == ATA_REG_DATA { read_data(n, cur(n), size / 8) } else { ide_port_read(port) };
    }
    ide_bus_master_read(port, size)
}

#[no_mangle]
pub unsafe fn ide_core_write(port: i32, size: i32, value: i32) {
    if let Some((n, reg)) = decode_core(port) {
        match reg {
            ATA_REG_DATA => write_data_port(n, cur(n), value, size / 8),
            ATA_REG_COMMAND => {
                let d = cur(n);
                rg(n, d).status &= !(ATA_SR_ERR | ATA_SR_DF);
                ata_command(n, d, value & 0xFF);
                device_lower_irq(channel(n).irq as u8);
            },
            8 => {
                let value = value & 0xFF;
                if value & ATA_CR_SRST != 0 {
                    device_lower_irq(channel(n).irq as u8);
                    device_reset(n, 0);
                    device_reset(n, 1);
                }
                channel(n).device_control = value;
            },
            _ => ide_port_write(port, value),
        }
        return;
    }
    let Some((n, reg)) = bus_master(port) else { return };
    let write_command8 = |n: usize, value: i32| {
        let c = channel(n);
        let old = c.dma_command;
        c.dma_command = value & 0x09;
        if old & 1 == value & 1 {
            return;
        }
        if value & 1 == 0 {
            c.dma_status &= !1;
            return;
        }
        c.dma_status |= 1;
        let d = cur(n);
        match dv(n, d).f.current_command {
            CMD_READ_DMA | CMD_READ_DMA_EXT => do_ata_read_sectors_dma(n, d),
            CMD_WRITE_DMA | CMD_WRITE_DMA_EXT => do_ata_write_sectors_dma(n, d),
            CMD_PACKET => do_atapi_dma(n, d),
            _ => {
                let c = channel(n);
                c.dma_status &= !1;
                c.dma_status |= 2;
                push_irq(n);
            },
        }
    };
    match (reg, size) {
        (0, 32) => {
            write_command8(n, value & 0xFF);
            channel(n).dma_status &= !(value >> 16 & 6);
        },
        (0, _) => write_command8(n, value & 0xFF),
        _ => ide_bus_master_write(port, size, value),
    }
}

/// rep insw / insd into guest memory at `phys` (ide.js's read_data_block); the items moved
pub unsafe fn core_read_block(port: i32, phys: u32, count: u32, size: u32) -> u32 {
    let Some((n, ATA_REG_DATA)) = decode_core(port) else { return 0 };
    let d = cur(n);
    let drive = dv(n, d);
    let dp = drive.f.data_pointer;
    if dp >= drive.f.data_end || dp % size as i32 != 0 {
        return 0;
    }
    let items = count.min(((drive.f.data_end - dp) as u32) / size);
    if items == 0 {
        return 0;
    }
    let bytes = (items * size) as usize;
    let from = dp as usize;
    let end = (from + bytes).min(drive.data.len());
    std::ptr::copy_nonoverlapping(drive.data[from..end].as_ptr(), mem8.add(phys as usize), end - from);
    drive.f.data_pointer += bytes as i32;
    if drive.f.data_pointer >= drive.f.data_end {
        read_end(n, d);
    }
    items
}

/// rep outsw / outsd from guest memory at `phys` (ide.js's write_data_block); the items moved
pub unsafe fn core_write_block(port: i32, phys: u32, count: u32, size: u32) -> u32 {
    let Some((n, ATA_REG_DATA)) = decode_core(port) else { return 0 };
    let d = cur(n);
    let drive = dv(n, d);
    let dp = drive.f.data_pointer;
    if dp >= drive.f.data_end || dp % size as i32 != 0 {
        return 0;
    }
    let items = count.min(((drive.f.data_end - dp) as u32) / size);
    if items == 0 {
        return 0;
    }
    let bytes = (items * size) as usize;
    let src = std::slice::from_raw_parts(mem8.add(phys as usize), bytes);
    drive.put(dp as usize, src);
    drive.f.data_pointer += bytes as i32;
    if drive.f.data_pointer == drive.f.data_end {
        write_end(n, d);
    }
    items
}

// ---------------------------------------------------------------------------------------------------------------
// ide.js's side

/// A drive's fields (DriveFields)
#[no_mangle]
pub unsafe fn ide_drive_fields(n: i32, d: i32) -> u32 { &raw mut dv(n as usize & 1, d as usize & 1).f as u32 }

/// The drive's data buffer (its size: ide_data_size); `size` >= 0 makes it that size first (a restored snapshot)
#[no_mangle]
pub unsafe fn ide_data(n: i32, d: i32, size: i32) -> u32 {
    let drive = dv(n as usize & 1, d as usize & 1);
    if size >= 0 {
        drive.data = vec![0; size as usize];
    }
    drive.data.as_mut_ptr() as u32
}

#[no_mangle]
pub unsafe fn ide_data_size(n: i32, d: i32) -> u32 { dv(n as usize & 1, d as usize & 1).data.len() as u32 }

/// Room for the `length` bytes of read `id` (when that read is still the one waited for)
#[no_mangle]
pub unsafe fn ide_io_buffer(n: i32, d: i32, length: i32) -> u32 {
    let drive = dv(n as usize & 1, d as usize & 1);
    drive.io.clear();
    drive.io.resize(length.max(0) as usize, 0);
    drive.io.as_mut_ptr() as u32
}

/// Read `id` is done, its bytes in ide_io_buffer
#[no_mangle]
pub unsafe fn ide_read_done(n: i32, d: i32, id: i32) {
    let (n, d) = (n as usize & 1, d as usize & 1);
    let drive = dv(n, d);
    let Some((pending_id, what)) = drive.pending
    else {
        return;
    };
    if pending_id != id {
        return;
    }
    drive.pending = None;
    match what {
        Pending::AtaRead { cmd, count, byte_count, is_single } => {
            drive.data_set_io();
            rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
            drive.f.data_end = if is_single { 512 } else { byte_count.min(drive.f.sectors_per_drq * 512) };
            let spt = drive.f.sectors_per_track;
            ata_advance(n, d, cmd, if is_single { 1 } else { count.min(spt) });
            push_irq(n);
        },
        Pending::AtaReadDma { count } => {
            let mut prdt = channel(n).prdt_addr as u32;
            let mut offset = 0usize;
            loop {
                let addr = phys_read32(prdt) as u32;
                let mut prd_count = phys_read16(prdt + 4) as usize;
                let end = phys_read8(prdt + 7) & 0x80 != 0;
                if prd_count == 0 {
                    prd_count = 0x10000;
                }
                let io = &dv(n, d).io;
                let from = offset.min(io.len());
                let to = (offset + prd_count).min(io.len());
                guest_write(addr, &io[from..to]);
                offset += prd_count;
                prdt = prdt.wrapping_add(8);
                if end || offset > (1 << 28) {
                    break;
                }
            }
            let cmd = dv(n, d).f.current_command;
            ata_advance(n, d, cmd, count);
            rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC;
            channel(n).dma_status &= !1;
            dv(n, d).f.current_command = -1;
            push_irq(n);
        },
        Pending::AtapiRead { req_length } => {
            drive.data_set_io();
            let r = rg(n, d);
            r.status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
            r.sector_count = r.sector_count & !7 | 2;
            push_irq(n);
            let drive = dv(n, d);
            drive.f.data_end = (req_length & !3).min(drive.f.data_length);
            let r = rg(n, d);
            r.lba_mid = drive.f.data_end & 0xFF;
            r.lba_high = drive.f.data_end >> 8 & 0xFF;
        },
        Pending::AtapiReadDma => {
            let r = rg(n, d);
            r.status = ATA_SR_DRDY | ATA_SR_DSC | ATA_SR_DRQ;
            r.sector_count = r.sector_count & !7 | 2;
            drive.data_set_io();
            do_atapi_dma(n, d);
        },
        Pending::AtaWriteDma { .. } => {},
    }
}

/// DMA write `id` is in the image
#[no_mangle]
pub unsafe fn ide_write_done(n: i32, d: i32, id: i32) {
    let (n, d) = (n as usize & 1, d as usize & 1);
    let drive = dv(n, d);
    let Some((pending_id, Pending::AtaWriteDma { count })) = drive.pending
    else {
        return;
    };
    if pending_id != id {
        return;
    }
    drive.pending = None;
    let cmd = drive.f.current_command;
    ata_advance(n, d, cmd, count);
    rg(n, d).status = ATA_SR_DRDY | ATA_SR_DSC;
    push_irq(n);
    channel(n).dma_status &= !1;
    dv(n, d).f.current_command = -1;
}

/// Reads and writes under way are forgotten
#[no_mangle]
pub unsafe fn ide_cancel(n: i32, d: i32) { cancel_io(n as usize & 1, d as usize & 1) }

/// The interrupt of the drive's channel, when interrupts are on (ide.js's push_irq)
#[no_mangle]
pub unsafe fn ide_push_irq(n: i32) { push_irq(n as usize & 1) }

/// A drive as a new one (ide.js makes its interfaces; a restored old snapshot makes them again)
#[no_mangle]
pub unsafe fn ide_drive_init(n: i32, d: i32, is_atapi: bool) {
    let drive = dv(n as usize & 1, d as usize & 1);
    drive.f = DriveFields {
        is_atapi: is_atapi as i32,
        has_disk: 0,
        sector_size: if is_atapi { 2048 } else { 512 },
        sectors_per_drq: 0x80,
        head_count: is_atapi as i32,
        sectors_per_track: 0,
        cylinder_count: 0,
        data_pointer: 0,
        data_length: 0,
        data_end: 0,
        current_command: -1,
        current_atapi_command: -1,
        atapi_sense_key: 0,
        atapi_add_sense: 0,
        medium_changed: 0,
        core: 1,
        sector_count: 0.0,
        write_dest: 0.0,
        disk_bytes: 0.0,
    };
    drive.data = vec![0; 64 * 1024];
    drive.io = Vec::new();
    drive.pending = None;
}
