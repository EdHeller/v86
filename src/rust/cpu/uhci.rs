// A UHCI USB 1.1 host controller (PIIX3's), as src/usb/uhci.js: its I/O registers, the root ports, and the
// schedule, a frame of 1 ms of the machine clock at a time (the frame list's entry, its queue heads and transfer
// descriptors in guest memory). The devices stay JavaScript (src/usb/usb_device.js: the tablet, the USB stick):
// each transaction is one call (usb_transaction) to the device that has the TD's address on an enabled port.
// uhci.js keeps the devices, attaching them, the PCI function and the snapshot format; it serves on i440FX
// machines (Q35's interrupt routing is JavaScript's: there uhci.js runs the controller itself).

use crate::cpu::cpu::js;
use crate::cpu::global_pointers::memory_size;
use crate::cpu::memory::mem8;

const IO_SIZE: i32 = 32;
pub const PORTS: usize = 2;

const R_USBCMD: i32 = 0x00;
const R_USBSTS: i32 = 0x02;
const R_USBINTR: i32 = 0x04;
const R_FRNUM: i32 = 0x06;
const R_FLBASEADD: i32 = 0x08;
const R_SOFMOD: i32 = 0x0C;
const R_PORTSC1: i32 = 0x10;

const CMD_RS: i32 = 1 << 0;
const CMD_HCRESET: i32 = 1 << 1;
const CMD_GRESET: i32 = 1 << 2;
const CMD_EGSM: i32 = 1 << 3;
const CMD_FGR: i32 = 1 << 4;
const CMD_MASK: i32 = 0xFF;
const STS_USBINT: i32 = 1 << 0;
const STS_ERROR: i32 = 1 << 1;
const STS_RD: i32 = 1 << 2;
const STS_HSE: i32 = 1 << 3;
const STS_HCPE: i32 = 1 << 4;
const STS_HALTED: i32 = 1 << 5;
const INTR_TIMEOUT_CRC: i32 = 1 << 0;
const INTR_RESUME: i32 = 1 << 1;
const INTR_IOC: i32 = 1 << 2;
const INTR_SHORT: i32 = 1 << 3;
const PORT_CCS: i32 = 1 << 0;
const PORT_CSC: i32 = 1 << 1;
const PORT_PE: i32 = 1 << 2;
const PORT_PEC: i32 = 1 << 3;
const PORT_RD: i32 = 1 << 6;
const PORT_RESERVED: i32 = 1 << 7;
const PORT_LSDA: i32 = 1 << 8;
const PORT_PR: i32 = 1 << 9;
const PORT_SUSPEND: i32 = 1 << 12;
const PORT_WRITE_CLEAR: i32 = PORT_CSC | PORT_PEC;
const PORT_WRITABLE: i32 = PORT_PE | PORT_RD | PORT_PR | PORT_SUSPEND;
const LINK_TERMINATE: u32 = 1;
const LINK_QH: u32 = 2;
const LINK_DEPTH: u32 = 4;
const TD_ACTLEN: u32 = 0x7FF;
const TD_BITSTUFF: u32 = 1 << 17;
const TD_CRC_TIMEOUT: u32 = 1 << 18;
const TD_NAK: u32 = 1 << 19;
const TD_BABBLE: u32 = 1 << 20;
const TD_BUFFER_ERROR: u32 = 1 << 21;
const TD_STALLED: u32 = 1 << 22;
const TD_ACTIVE: u32 = 1 << 23;
const TD_IOC: u32 = 1 << 24;
const TD_ERRORS_SHIFT: u32 = 27;
const TD_SPD: u32 = 1 << 29;
const TD_STATUS: u32 = TD_BITSTUFF | TD_CRC_TIMEOUT | TD_NAK | TD_BABBLE | TD_BUFFER_ERROR | TD_STALLED;
const PID_SETUP: u32 = 0x2D;
const PID_IN: u32 = 0x69;
const PID_OUT: u32 = 0xE1;
// what usb_transaction answers, besides a length
const USB_NODEV: i32 = -1;
const USB_NAK: i32 = -2;
const USB_STALL: i32 = -3;
const USB_BABBLE: i32 = -4;

const FRAME_TDS: i32 = 256;
const FRAME_BYTES: i32 = 16 * 1024;
const FRAMES_PER_ROUND: i32 = 16;
const FRAMES_BEHIND: f64 = 100.0;

// (the layout is read by src/usb/uhci.js)
#[repr(C)]
pub struct Uhci {
    /// the machine time of the next frame (while running)
    next_frame: f64, // 0
    cmd: i32,        // 8: 0
    status: i32,     // 1
    /// what set USBINT: 1 IOC, 2 a short packet
    status2: i32, // 2
    intr: i32,          // 3
    frnum: i32,         // 4
    flbase: i32,        // 5
    sofmod: i32,        // 6
    portsc: [i32; PORTS], // 7, 8
    irq_level: i32,     // 9
    present: i32,       // 10
    io_base: i32,       // 11
    pci_id: i32,        // 12
    /// a device is attached to the port (JavaScript has it), and it is a low speed one
    attached: [i32; PORTS], // 13, 14
    low_speed: [i32; PORTS], // 15, 16
    /// ports whose device a reset of the bus or the port put back to its default state: JavaScript resets the device
    /// before its next transaction
    reset_pending: i32, // 17
    frame_bytes: i32,   // 18
}

static mut UHCI: Uhci = Uhci {
    next_frame: 0.0,
    cmd: 0,
    status: STS_HALTED,
    status2: 0,
    intr: 0,
    frnum: 0,
    flbase: 0,
    sofmod: 64,
    portsc: [0; PORTS],
    irq_level: 0,
    present: 0,
    io_base: 0,
    pci_id: 0,
    attached: [0; PORTS],
    low_speed: [0; PORTS],
    reset_pending: 0,
    frame_bytes: 0,
};

#[allow(static_mut_refs)]
fn uhci() -> &'static mut Uhci { unsafe { &mut UHCI } }

#[no_mangle]
pub fn get_uhci_addr() -> u32 { &raw mut UHCI as u32 }

/// The controller (not EHCI's companions) at io_base, PCI function pci_id
#[no_mangle]
pub unsafe fn uhci_init(io_base: i32, pci_id: i32) {
    let u = uhci();
    u.present = 1;
    u.io_base = io_base;
    u.pci_id = pci_id;
}

unsafe fn dma_read32(address: u32) -> u32 {
    if address as u64 + 4 > *memory_size as u64 {
        return 0;
    }
    (mem8.offset(address as isize) as *const u32).read_unaligned()
}

unsafe fn dma_write32(address: u32, value: u32) {
    if address as u64 + 4 <= *memory_size as u64 {
        crate::cpu::memory::write32(address, value as i32);
    }
}

impl Uhci {
    unsafe fn update_irq(&mut self) {
        let level = self.status2 & 1 != 0 && self.intr & INTR_IOC != 0
            || self.status2 & 2 != 0 && self.intr & INTR_SHORT != 0
            || self.status & STS_ERROR != 0 && self.intr & INTR_TIMEOUT_CRC != 0
            || self.status & STS_RD != 0 && self.intr & INTR_RESUME != 0
            || self.status & (STS_HSE | STS_HCPE) != 0;
        if level as i32 == self.irq_level {
            return;
        }
        self.irq_level = level as i32;
        super::pci::set_irq(self.pci_id as usize & 0xFF, level);
    }

    unsafe fn reset(&mut self) {
        self.cmd = 0;
        self.status = STS_HALTED;
        self.status2 = 0;
        self.intr = 0;
        self.frnum = 0;
        self.flbase = 0;
        self.sofmod = 64;
        self.next_frame = 0.0;
        for i in 0..PORTS {
            self.portsc[i] = 0;
            if self.attached[i] != 0 {
                self.reset_pending |= 1 << i;
                self.portsc[i] = PORT_CCS | PORT_CSC | if self.low_speed[i] != 0 { PORT_LSDA } else { 0 };
            }
        }
        self.update_irq();
    }

    fn register(&self, offset: i32) -> i32 {
        match offset {
            R_USBCMD => self.cmd,
            R_USBSTS => self.status,
            R_USBINTR => self.intr,
            R_FRNUM => self.frnum,
            R_FLBASEADD => self.flbase,
            R_SOFMOD => self.sofmod,
            _ if offset >= R_PORTSC1 && offset < R_PORTSC1 + 2 * PORTS as i32 => {
                let port = ((offset - R_PORTSC1) >> 1) as usize;
                // (bit 7 always reads 1: drivers count the ports by it)
                self.portsc[port]
                    | PORT_RESERVED
                    | if self.portsc[port] & PORT_CCS != 0 && self.portsc[port] & PORT_LSDA == 0 { 1 << 4 } else { 0 }
            },
            _ => 0,
        }
    }

    unsafe fn write_register(&mut self, offset: i32, bits: u32, mask: u32) {
        let merge = |old: i32| (old as u32 & !mask | bits & mask) as i32;
        match offset {
            R_USBCMD => {
                let value = merge(self.cmd) & CMD_MASK;
                if value & CMD_HCRESET != 0 {
                    self.reset();
                    return;
                }
                if value & CMD_GRESET != 0 && self.cmd & CMD_GRESET == 0 {
                    // the bus's reset: every device to its default state
                    for i in 0..PORTS {
                        if self.attached[i] != 0 {
                            self.reset_pending |= 1 << i;
                        }
                    }
                }
                let was_running = self.cmd & CMD_RS != 0;
                self.cmd = value;
                if value & CMD_RS != 0 && !was_running {
                    self.status &= !STS_HALTED;
                    self.next_frame = js::microtick() + 1.0;
                }
                else if value & CMD_RS == 0 && was_running {
                    self.status |= STS_HALTED;
                }
                if value & CMD_FGR != 0 && self.status & STS_RD != 0 {
                    self.status &= !STS_RD;
                }
            },
            R_USBSTS => {
                self.status &= !((bits & mask) as i32 & (STS_USBINT | STS_ERROR | STS_RD | STS_HSE | STS_HCPE));
                if self.status & STS_USBINT == 0 {
                    self.status2 = 0;
                }
                self.update_irq();
            },
            R_USBINTR => {
                self.intr = merge(self.intr) & 0xF;
                self.update_irq();
            },
            R_FRNUM => {
                // (only while halted)
                if self.status & STS_HALTED != 0 {
                    self.frnum = merge(self.frnum) & 0x7FF;
                }
            },
            R_FLBASEADD => self.flbase = merge(self.flbase) & !0xFFF,
            R_SOFMOD => self.sofmod = merge(self.sofmod) & 0x7F,
            _ => {
                let port = ((offset - R_PORTSC1) >> 1) as usize;
                let old = self.portsc[port];
                let (bits, mask) = (bits as i32, mask as i32);
                let mut value = old & !(bits & mask & PORT_WRITE_CLEAR);
                value = value & !(PORT_WRITABLE & mask) | bits & mask & PORT_WRITABLE;
                if value & PORT_PR != 0 && old & PORT_PR == 0 && self.attached[port] != 0 {
                    // a port reset: the device to its default state, the port disabled
                    self.reset_pending |= 1 << port;
                    value &= !PORT_PE;
                }
                if value & PORT_CCS == 0 {
                    value &= !PORT_PE;
                }
                if value & PORT_PR != 0 {
                    value &= !PORT_PE;
                }
                self.portsc[port] = value;
            },
        }
    }

    unsafe fn host_error(&mut self) {
        self.status |= STS_HSE | STS_HALTED;
        self.cmd &= !CMD_RS;
        self.update_irq();
    }

    /// the ports a device answers on: enabled and not suspended (whether a device is there, JavaScript knows)
    fn port_mask(&self) -> i32 {
        let mut mask = 0;
        for i in 0..PORTS {
            if self.portsc[i] & PORT_PE != 0 && self.portsc[i] & PORT_SUSPEND == 0 {
                mask |= 1 << i;
            }
        }
        mask
    }

    /// one TD: (completed, the next link, interrupts: 1 IOC, 2 short packet)
    unsafe fn process_td(&mut self, address: u32) -> (bool, u32, i32) {
        let link = dma_read32(address);
        let mut control = dma_read32(address + 4);
        let token = dma_read32(address + 8);
        let buffer = dma_read32(address + 12);
        if control & TD_ACTIVE == 0 {
            return (false, link, 0);
        }
        let pid = token & 0xFF;
        let device_address = (token >> 8 & 0x7F) as i32;
        let endpoint = (token >> 15 & 0xF) as i32;
        let max_length = ((token >> 21) + 1 & 0x7FF) as i32;
        let mut interrupts = 0;

        if pid != PID_IN && pid != PID_OUT && pid != PID_SETUP {
            // (a PID that is not a token: the controller's process error)
            self.status |= STS_HCPE;
            self.cmd &= !CMD_RS;
            self.status |= STS_HALTED;
            self.update_irq();
            return (false, link, 0);
        }

        // (the tablet and the disks' data stages are the core's: asked here, their data straight in guest memory)
        let mut pending = self.reset_pending as u32;
        let tablet = match super::usb_tablet::guest(buffer, max_length as u32) {
            Some(data) => super::usb_tablet::route(self.port_mask() as u32, device_address, pid as i32, endpoint, data, max_length as usize, &mut pending)
                .or_else(|| super::usb_storage::route(self.port_mask() as u32, device_address, pid as i32, endpoint, data, max_length as usize)),
            None => None,
        };
        self.reset_pending = pending as i32;
        let result = match tablet {
            Some(result) => {
                if pid == PID_IN && result > 0 {
                    crate::jit::jit_dirty_cache(buffer, buffer + result as u32);
                }
                result
            },
            None => js::usb_transaction(self.port_mask(), device_address, pid as i32, endpoint, buffer, max_length),
        };

        if result == USB_NODEV {
            // no answer: a timeout, the error counter down, at 0 the TD is retired
            let mut errors = control >> TD_ERRORS_SHIFT & 3;
            control |= TD_CRC_TIMEOUT;
            if errors != 0 {
                errors -= 1;
                if errors == 0 {
                    control &= !TD_ACTIVE;
                    self.status |= STS_ERROR;
                    if control & TD_IOC != 0 {
                        interrupts |= 1;
                    }
                    self.update_irq();
                }
            }
            control = control & !(3 << TD_ERRORS_SHIFT) | errors << TD_ERRORS_SHIFT;
            dma_write32(address + 4, control);
            return (false, link, interrupts);
        }
        if result == USB_NAK {
            // (the TD stays active; it is tried again in a later frame)
            return (false, link, interrupts);
        }

        let mut complete = |control: &mut u32, status: u32, length: i32, uhci: &mut Uhci| {
            *control = *control & !(TD_STATUS | TD_ACTIVE | TD_ACTLEN) | status | (length - 1) as u32 & TD_ACTLEN;
            if status & (TD_STALLED | TD_BABBLE | TD_CRC_TIMEOUT) != 0 {
                uhci.status |= STS_ERROR;
                uhci.update_irq();
            }
            if *control & TD_IOC != 0 {
                interrupts |= 1;
            }
        };

        if result == USB_STALL {
            complete(&mut control, TD_STALLED, 0, self);
            dma_write32(address + 4, control);
            return (false, link, interrupts);
        }
        if result == USB_BABBLE {
            complete(&mut control, TD_BABBLE | TD_STALLED, max_length, self);
            dma_write32(address + 4, control);
            return (false, link, interrupts);
        }
        let length = result;
        self.frame_bytes += length;
        complete(&mut control, 0, length, self);
        dma_write32(address + 4, control);
        if pid == PID_IN && length < max_length && control & TD_SPD != 0 {
            // a short packet: the queue stops here (the driver goes on with it)
            interrupts |= 2;
            return (false, link, interrupts);
        }
        (true, link, interrupts)
    }

    unsafe fn process_frame(&mut self) {
        let command = super::pci::command(self.pci_id as usize & 0xFF);
        if command & 4 == 0 {
            // not a bus master: the frames go by
            self.frnum = self.frnum + 1 & 0x7FF;
            return;
        }
        let entry = dma_read32((self.flbase as u32).wrapping_add(4 * (self.frnum as u32 & 0x3FF)));
        let mut link = entry;
        let mut tds = 0;
        let mut interrupts = 0;
        self.frame_bytes = 0;
        // queue heads seen in this frame: the TDs completed when last seen
        let mut seen: Vec<(u32, i32)> = Vec::new();
        let mut completed = 0;
        let (mut qh, mut qh_head) = (0u32, 0u32);
        while link & LINK_TERMINATE == 0 && tds < FRAME_TDS && self.frame_bytes < FRAME_BYTES {
            if link & LINK_QH != 0 {
                qh = link & !0xF;
                if let Some(e) = seen.iter_mut().find(|e| e.0 == qh) {
                    if e.1 == completed {
                        break;
                    }
                    e.1 = completed;
                }
                else {
                    seen.push((qh, completed));
                }
                qh_head = dma_read32(qh);
                let element = dma_read32(qh + 4);
                if element & LINK_TERMINATE != 0 {
                    link = qh_head;
                    qh = 0;
                    continue;
                }
                if element & LINK_QH != 0 {
                    link = element;
                    qh = 0;
                    continue;
                }
                link = element;
            }
            let td = link & !0xF;
            tds += 1;
            let (done, next, td_interrupts) = self.process_td(td);
            interrupts |= td_interrupts;
            if done {
                completed += 1;
                if qh != 0 {
                    // the queue goes on: its element the next TD
                    dma_write32(qh + 4, next);
                    if next & LINK_DEPTH == 0 || next & (LINK_TERMINATE | LINK_QH) != 0 {
                        link = qh_head;
                        qh = 0;
                    }
                    else {
                        link = next;
                    }
                }
                else {
                    link = next;
                }
            }
            else {
                link = if qh != 0 { qh_head } else { next };
                qh = 0;
            }
        }
        self.frnum = self.frnum + 1 & 0x7FF;
        if interrupts != 0 {
            self.status |= STS_USBINT;
            self.status2 |= interrupts;
            self.update_irq();
        }
    }

    /// the schedule: the frames that fell due; returns the milliseconds until the next
    unsafe fn timer(&mut self, now: f64) -> f64 {
        if self.cmd & CMD_RS == 0 || self.cmd & CMD_EGSM != 0 {
            return 100.0;
        }
        if now - self.next_frame > FRAMES_BEHIND {
            // (far behind: the frames of the gap are not processed)
            let skipped = (now - self.next_frame).floor() - FRAMES_PER_ROUND as f64;
            self.frnum = (self.frnum as i64 + skipped as i64 & 0x7FF) as i32;
            self.next_frame += skipped;
        }
        let mut i = 0;
        while i < FRAMES_PER_ROUND && now >= self.next_frame {
            self.process_frame();
            self.next_frame += 1.0;
            if self.cmd & CMD_RS == 0 {
                return 100.0;
            }
            i += 1;
        }
        (self.next_frame - now).max(0.0)
    }
}

/// the register offset of a port of the controller's window, at its base or where the BIOS moved BAR4
#[inline]
unsafe fn offset_of(port: i32) -> Option<i32> {
    let u = uhci();
    if u.present == 0 || super::parallel::WORKER || super::pci::is_q35() {
        return None;
    }
    if port >= u.io_base && port < u.io_base + IO_SIZE {
        return Some(port - u.io_base);
    }
    let bar = super::pci::io_bar(u.pci_id as usize & 0xFF, 4);
    if bar >= 0x1000 && bar != u.io_base && port >= bar && port < bar + IO_SIZE {
        return Some(port - bar);
    }
    None
}

#[inline]
pub unsafe fn serves(port: i32) -> bool { offset_of(port).is_some() }

/// (register start, size) of an offset
fn register_at(offset: i32) -> Option<(i32, i32)> {
    match offset {
        0x00..=0x01 => Some((R_USBCMD, 2)),
        0x02..=0x03 => Some((R_USBSTS, 2)),
        0x04..=0x05 => Some((R_USBINTR, 2)),
        0x06..=0x07 => Some((R_FRNUM, 2)),
        0x08..=0x0B => Some((R_FLBASEADD, 4)),
        0x0C => Some((R_SOFMOD, 1)),
        0x10..=0x13 => Some((offset & !1, 2)),
        _ => None,
    }
}

#[no_mangle]
pub unsafe fn uhci_port_read(port: i32, size: i32) -> i32 {
    let Some(offset) = offset_of(port) else { return -1 };
    let u = uhci();
    let mut value = 0u32;
    for i in 0..size / 8 {
        if let Some((start, _)) = register_at(offset + i) {
            value |= (u.register(start) as u32 >> ((offset + i - start) << 3) & 0xFF) << (i << 3);
        }
    }
    value as i32
}

#[no_mangle]
pub unsafe fn uhci_port_write(port: i32, size: i32, value: i32) {
    let Some(offset) = offset_of(port) else { return };
    let u = uhci();
    let size = size / 8;
    let mut i = 0;
    while i < size {
        let Some((start, length)) = register_at(offset + i) else {
            i += 1;
            continue;
        };
        let (mut mask, mut bits) = (0u32, 0u32);
        while i < size && offset + i < start + length {
            let shift = (offset + i - start) << 3;
            mask |= 0xFF << shift;
            bits |= (value as u32 >> (i << 3) & 0xFF) << shift;
            i += 1;
        }
        u.write_register(start, bits, mask);
    }
}

/// A device attached to a port (attached) or taken out
#[no_mangle]
pub unsafe fn uhci_attach(port: i32, attached: bool, low_speed: bool) {
    let u = uhci();
    let p = port as usize & 1;
    u.attached[p] = attached as i32;
    u.low_speed[p] = low_speed as i32;
    if attached {
        u.reset_pending |= 1 << p;
        u.portsc[p] = u.portsc[p] & !(PORT_LSDA | PORT_PE) | PORT_CCS | PORT_CSC | if low_speed { PORT_LSDA } else { 0 };
    }
    else {
        u.portsc[p] =
            u.portsc[p] & !(PORT_CCS | PORT_PE | PORT_LSDA) | PORT_CSC | if u.portsc[p] & PORT_PE != 0 { PORT_PEC } else { 0 };
    }
    // (a connect while the bus is suspended is a resume)
    if u.cmd & CMD_EGSM != 0 {
        u.status |= STS_RD;
        u.update_irq();
    }
}

#[no_mangle]
pub unsafe fn uhci_reset() { uhci().reset() }

#[no_mangle]
pub unsafe fn uhci_update_irq() { uhci().update_irq() }

/// The schedule's timer, from the core's timer pass
pub unsafe fn timer(now: f64) -> f64 {
    let u = uhci();
    if u.present == 0 || super::pci::is_q35() {
        return 100.0;
    }
    u.timer(now)
}
