// An xHCI USB 3 host controller (QEMU's qemu-xhci, 1B36:000D), as src/usb/xhci.js: the memory-mapped registers
// (capability, operational with the ports, runtime with interrupter 0, doorbells, extended capabilities), the
// command ring, the event ring, the device slots and their contexts, the transfer rings and their TDs, retried
// each millisecond of the machine clock while a device NAKs. The devices stay JavaScript: a transaction is one call
// (usb_transaction, as UHCI's, with the port's bit and no address) to the device in a physical port, its data at an address of this module's memory (guest
// memory or the immediate data of a TRB). xhci.js keeps the devices, attaching them (it gives the device's speed),
// the PCI function (decoding, the power management capability) and the snapshot format, through the plugin
// handle's core_xhci. INTx only: v86's PCI offers no MSI. On Q35 xhci.js runs the controller itself.

use crate::cpu::cpu::js;
use crate::cpu::global_pointers::memory_size;
use crate::cpu::memory::mem8;

const MMIO_SIZE: u32 = 0x4000;
const OP: u32 = 0x40;
const PORTS_BASE: u32 = OP + 0x400;
const RUNTIME: u32 = 0x1000;
const DOORBELLS: u32 = 0x2000;
const XECP: u32 = 0x3000;
pub const XHCI_PORTS: usize = 4;
const ROOT_PORTS: usize = 2 * XHCI_PORTS;
const USB3_FIRST: usize = 1;
const USB2_FIRST: usize = XHCI_PORTS + 1;
const MAX_SLOTS: usize = 32;

const HCSPARAMS1: u32 = MAX_SLOTS as u32 | 1 << 8 | (ROOT_PORTS as u32) << 24;
const HCSPARAMS2: u32 = 0x0000000F;
const HCCPARAMS1: u32 = 1 | (XECP >> 2) << 16;
const LEGSUP_BIOS_OWNED: u32 = 1 << 16;
const LEGSUP_OS_OWNED: u32 = 1 << 24;
const EXTENDED: [&[u32]; 3] = [
    &[1 | 4 << 8, 0],
    &[2 | 4 << 8 | 0x03 << 24, 0x20425355, USB3_FIRST as u32 | (XHCI_PORTS as u32) << 8, 0],
    &[2 | 0x02 << 24, 0x20425355, USB2_FIRST as u32 | (XHCI_PORTS as u32) << 8, 0],
];

const R_USBCMD: u32 = 0x00;
const R_USBSTS: u32 = 0x04;
const R_PAGESIZE: u32 = 0x08;
const R_DNCTRL: u32 = 0x14;
const R_CRCR: u32 = 0x18;
const R_CRCR_HIGH: u32 = 0x1C;
const R_DCBAAP: u32 = 0x30;
const R_DCBAAP_HIGH: u32 = 0x34;
const R_CONFIG: u32 = 0x38;
const CMD_RS: u32 = 1 << 0;
const CMD_HCRST: u32 = 1 << 1;
const CMD_INTE: u32 = 1 << 2;
const CMD_CSS: u32 = 1 << 8;
const CMD_CRS: u32 = 1 << 9;
const CMD_MASK: u32 = 0x3F0F;
const STS_HCH: u32 = 1 << 0;
const STS_HSE: u32 = 1 << 2;
const STS_EINT: u32 = 1 << 3;
const STS_PCD: u32 = 1 << 4;
const STS_SRE: u32 = 1 << 10;
const STS_HCE: u32 = 1 << 12;
const CRCR_RCS: u32 = 1 << 0;
const CRCR_CS: u32 = 1 << 1;
const CRCR_CA: u32 = 1 << 2;
const CRCR_CRR: u32 = 1 << 3;
const PORT_CCS: u32 = 1 << 0;
const PORT_PED: u32 = 1 << 1;
const PORT_PR: u32 = 1 << 4;
const PORT_PLS_SHIFT: u32 = 5;
const PORT_PLS: u32 = 0xF << PORT_PLS_SHIFT;
const PORT_PP: u32 = 1 << 9;
const PORT_SPEED_SHIFT: u32 = 10;
const PORT_LWS: u32 = 1 << 16;
const PORT_CSC: u32 = 1 << 17;
const PORT_PEC: u32 = 1 << 18;
const PORT_WRC: u32 = 1 << 19;
const PORT_OCC: u32 = 1 << 20;
const PORT_PRC: u32 = 1 << 21;
const PORT_PLC: u32 = 1 << 22;
const PORT_CEC: u32 = 1 << 23;
const PORT_WAKE: u32 = 7 << 25;
const PORT_WPR: u32 = 1 << 31;
const PORT_CHANGES: u32 = PORT_CSC | PORT_PEC | PORT_WRC | PORT_OCC | PORT_PRC | PORT_PLC | PORT_CEC;
const PLS_U0: u32 = 0;
const PLS_U3: u32 = 3;
const PLS_DISABLED: u32 = 4;
const PLS_RX_DETECT: u32 = 5;
const PLS_POLLING: u32 = 7;
const PLS_RESUME: u32 = 15;
// the port's speed IDs by the device's speed (low, full, high, super)
const SPEED_IDS: [u32; 4] = [2, 1, 3, 4];
const SPEED_SUPER: i32 = 3;
const IMAN_IP: u32 = 1 << 0;
const IMAN_IE: u32 = 1 << 1;
const ERDP_EHB: u32 = 1 << 3;
const TRB_CYCLE: u32 = 1 << 0;
const TRB_TC: u32 = 1 << 1;
const TRB_ISP: u32 = 1 << 2;
const TRB_CH: u32 = 1 << 4;
const TRB_IOC: u32 = 1 << 5;
const TRB_IDT: u32 = 1 << 6;
const TRB_BSR: u32 = 1 << 9;
const TRB_DC: u32 = 1 << 9;
const TRB_DIR_IN: u32 = 1 << 16;
const TRB_NORMAL: u32 = 1;
const TRB_SETUP: u32 = 2;
const TRB_DATA: u32 = 3;
const TRB_STATUS: u32 = 4;
const TRB_ISOCH: u32 = 5;
const TRB_LINK: u32 = 6;
const TRB_EVENT_DATA: u32 = 7;
const TRB_NOOP: u32 = 8;
const CMD_ENABLE_SLOT: u32 = 9;
const CMD_DISABLE_SLOT: u32 = 10;
const CMD_ADDRESS_DEVICE: u32 = 11;
const CMD_CONFIGURE_ENDPOINT: u32 = 12;
const CMD_EVALUATE_CONTEXT: u32 = 13;
const CMD_RESET_ENDPOINT: u32 = 14;
const CMD_STOP_ENDPOINT: u32 = 15;
const CMD_SET_TR_DEQUEUE: u32 = 16;
const CMD_RESET_DEVICE: u32 = 17;
const CMD_GET_PORT_BANDWIDTH: u32 = 21;
const CMD_NOOP: u32 = 23;
const EVENT_TRANSFER: u32 = 32;
const EVENT_COMMAND: u32 = 33;
const EVENT_PORT: u32 = 34;
const EVENT_HOST: u32 = 37;
const CC_SUCCESS: u32 = 1;
const CC_DATA_BUFFER: u32 = 2;
const CC_BABBLE: u32 = 3;
const CC_TRANSACTION: u32 = 4;
const CC_TRB: u32 = 5;
const CC_STALL: u32 = 6;
const CC_NO_SLOTS: u32 = 9;
const CC_SLOT_NOT_ENABLED: u32 = 11;
const CC_EP_NOT_ENABLED: u32 = 12;
const CC_SHORT_PACKET: u32 = 13;
const CC_PARAMETER: u32 = 17;
const CC_CONTEXT_STATE: u32 = 19;
const CC_EVENT_RING_FULL: u32 = 21;
const CC_COMMAND_RING_STOPPED: u32 = 24;
const CC_STOPPED: u32 = 26;
const SLOT_DEFAULT: u32 = 1;
const SLOT_ADDRESSED: u32 = 2;
const SLOT_CONFIGURED: u32 = 3;
const EP_DISABLED: u32 = 0;
const EP_RUNNING: u32 = 1;
const EP_HALTED: u32 = 2;
const EP_STOPPED: u32 = 3;
const EP_CONTROL: u32 = 4;
const EP_ISOCH_OUT: u32 = 1;
const EP_ISOCH_IN: u32 = 5;
const DOORBELL_BYTES: i32 = 64 * 1024;
const MS_BYTES: i32 = 1024 * 1024;
const LINK_LIMIT: usize = 32;
const TD_TRBS: usize = 512;
const PID_SETUP: i32 = 0x2D;
const PID_IN: i32 = 0x69;
const PID_OUT: i32 = 0xE1;
const USB_NODEV: i32 = -1;
const USB_NAK: i32 = -2;
const USB_STALL: i32 = -3;
const USB_BABBLE: i32 = -4;

#[derive(Clone, Copy, Default)]
struct Ring {
    dequeue: u64,
    ccs: u32,
}

#[derive(Clone, Copy, Default)]
struct Trb {
    address: u64,
    low: u32,
    high: u32,
    status: u32,
    control: u32,
    ty: u32,
    ccs: u32,
}

#[derive(Clone, Default)]
struct Td {
    trbs: Vec<Trb>,
    index: usize,
    done: Vec<u32>,
    short: i32,
    error: u32,
    error_index: i32,
    after: Ring,
}

#[derive(Clone, Default)]
struct Ep {
    dci: u32,
    ty: u32,
    max_packet: u32,
    interval: u32,
    ring: Ring,
    state: u32,
    td: Option<Td>,
}

#[derive(Clone, Default)]
struct Slot {
    port: i32,
    root: u32,
    context: u64,
    state: u32,
    gone: bool,
    endpoints: Vec<Option<Ep>>,
}

#[derive(Default)]
struct EventRing {
    start: u64,
    size: u32,
    enqueue: u32,
    pcs: u32,
}

#[derive(Default)]
pub struct Xhci {
    present: bool,
    pci_id: usize,
    /// where BAR0 is decoded, or -1
    mmio: i64,
    cmd: u32,
    status: u32,
    dnctrl: u32,
    crcr: [u32; 2],
    command_ring: Ring,
    crr: bool,
    dcbaap: [u32; 2],
    config: u32,
    iman: u32,
    imod: u32,
    erstsz: u32,
    erstba: [u32; 2],
    erdp: [u32; 2],
    event_ring: EventRing,
    mf_start: f64,
    next_ms: f64,
    budget: i32,
    slots: Vec<Option<Slot>>,
    /// endpoints with transfers to go on with: (slot, dci), in order
    active: Vec<(usize, u32)>,
    portsc: [u32; ROOT_PORTS + 1],
    legsup: u32,
    irq_level: bool,
    /// per physical port: a device, whether it is a SuperSpeed one, its speed on its root port
    attached: [bool; XHCI_PORTS],
    superspeed: [bool; XHCI_PORTS],
    speed: [i32; XHCI_PORTS],
    /// ports whose device a reset put back to its default state: JavaScript resets it before its next transaction
    reset_pending: u32,
}

static mut XHCI: Option<Xhci> = None;

#[allow(static_mut_refs)]
fn xhci() -> &'static mut Xhci { unsafe { XHCI.get_or_insert_with(Xhci::default) } }

/// a scratch buffer for the immediate data of TRBs (setup packets, IDT)
static mut SCRATCH: [u8; 8] = [0; 8];

unsafe fn guest_ptr(address: u64, length: u32) -> Option<u32> {
    if address + length as u64 > *memory_size as u64 {
        return None;
    }
    Some(mem8 as u32 + address as u32)
}

impl Xhci {
    unsafe fn dma_read(&mut self, address: u64, length: u32) -> Option<&'static [u8]> {
        match guest_ptr(address, length) {
            Some(p) => Some(std::slice::from_raw_parts(p as *const u8, length as usize)),
            None => {
                self.status |= STS_HSE;
                None
            },
        }
    }

    unsafe fn dma_write(&mut self, bytes: &[u8], address: u64) -> bool {
        match guest_ptr(address, bytes.len() as u32) {
            Some(p) => {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p as *mut u8, bytes.len());
                // (written behind the core's back: compiled code there is stale)
                crate::jit::jit_dirty_cache(address as u32, address as u32 + bytes.len() as u32);
                true
            },
            None => {
                self.status |= STS_HSE;
                false
            },
        }
    }

    unsafe fn write_dwords(&mut self, address: u64, words: &[u32]) {
        let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.dma_write(&bytes, address);
    }

    unsafe fn reset(&mut self) {
        self.cmd = 0;
        self.status = STS_HCH;
        self.dnctrl = 0;
        self.crcr = [0, 0];
        self.command_ring = Ring { dequeue: 0, ccs: 1 };
        self.crr = false;
        self.dcbaap = [0, 0];
        self.config = 0;
        self.iman = 0;
        self.imod = 0;
        self.erstsz = 0;
        self.erstba = [0, 0];
        self.erdp = [0, 0];
        self.event_ring = EventRing { start: 0, size: 0, enqueue: 0, pcs: 1 };
        self.mf_start = js::microtick();
        self.next_ms = 0.0;
        self.budget = 0;
        self.slots = vec![None; MAX_SLOTS + 1];
        self.active.clear();
        self.portsc = [0; ROOT_PORTS + 1];
        for port in 0..XHCI_PORTS {
            self.update_port(port, false);
        }
        self.update_irq();
    }

    fn root_port(&self, port: usize) -> usize { if self.superspeed[port] { USB3_FIRST + port } else { USB2_FIRST + port } }

    unsafe fn update_port(&mut self, port: usize, notify: bool) {
        for root in [USB3_FIRST + port, USB2_FIRST + port] {
            let connected = self.attached[port] && root == self.root_port(port);
            let before = self.portsc[root];
            let mut value = PORT_PP | before & PORT_WAKE;
            if connected {
                let speed = if root < USB2_FIRST { SPEED_SUPER } else { self.speed[port] };
                value |= PORT_CCS | SPEED_IDS[speed as usize & 3] << PORT_SPEED_SHIFT;
                value |= if speed == SPEED_SUPER { PORT_PED | PLS_U0 << PORT_PLS_SHIFT } else { PLS_POLLING << PORT_PLS_SHIFT };
            }
            else {
                value |= (if root < USB2_FIRST { PLS_RX_DETECT } else { PLS_DISABLED }) << PORT_PLS_SHIFT;
            }
            if (before & PORT_CCS != 0) != connected {
                value |= PORT_CSC;
            }
            self.portsc[root] = value | before & PORT_CHANGES;
            if notify && value & PORT_CSC != 0 {
                self.port_event(root);
            }
        }
    }

    unsafe fn port_event(&mut self, root: usize) {
        self.status |= STS_PCD;
        if self.status & STS_HCH == 0 {
            self.event((root as u32) << 24, 0, CC_SUCCESS << 24, EVENT_PORT << 10);
        }
    }

    unsafe fn write_port(&mut self, root: usize, value: u32) {
        let port = (root - 1) % XHCI_PORTS;
        let here = self.attached[port] && root == self.root_port(port);
        if value & (PORT_PR | PORT_WPR) != 0 {
            // a reset (a warm one: USB 3's): the device to its default state, the link up, enabled
            if here {
                self.reset_pending |= 1 << port;
                self.portsc[root] = self.portsc[root] & !(PORT_PLS | PORT_PR)
                    | PLS_U0 << PORT_PLS_SHIFT
                    | PORT_PED
                    | PORT_PRC
                    | if value & PORT_WPR != 0 && root < USB2_FIRST { PORT_WRC } else { 0 };
                self.port_event(root);
            }
            return;
        }
        let mut portsc = self.portsc[root] & !(value & PORT_CHANGES);
        // (PED written 1: the driver disables the port)
        if value & PORT_PED != 0 {
            portsc &= !PORT_PED;
        }
        if value & PORT_LWS != 0 {
            let pls = value >> PORT_PLS_SHIFT & 0xF;
            let before = portsc >> PORT_PLS_SHIFT & 0xF;
            // suspend (U3) and resume (U0: Port Link State Change); the others are not the driver's
            if pls == PLS_U3 && before == PLS_U0 {
                portsc = portsc & !PORT_PLS | PLS_U3 << PORT_PLS_SHIFT;
            }
            else if (pls == PLS_U0 || pls == PLS_RESUME) && before == PLS_U3 {
                portsc = portsc & !PORT_PLS | PLS_U0 << PORT_PLS_SHIFT | PORT_PLC;
                self.portsc[root] = portsc;
                self.port_event(root);
                return;
            }
        }
        self.portsc[root] = portsc & !PORT_WAKE | value & PORT_WAKE;
    }

    unsafe fn mfindex(&self) -> u32 {
        if self.status & STS_HCH != 0 { 0 } else { ((js::microtick() - self.mf_start) * 8.0).floor() as i64 as u32 & 0x3FFF }
    }

    unsafe fn register(&self, offset: u32) -> u32 {
        if offset < OP {
            return match offset {
                // CAPLENGTH, HCIVERSION 1.0
                0x00 => OP | 0x0100 << 16,
                0x04 => HCSPARAMS1,
                0x08 => HCSPARAMS2,
                0x10 => HCCPARAMS1,
                0x14 => DOORBELLS,
                0x18 => RUNTIME,
                _ => 0,
            };
        }
        if offset >= PORTS_BASE && offset < PORTS_BASE + 0x10 * ROOT_PORTS as u32 {
            return if offset & 0xF == 0 { self.portsc[((offset - PORTS_BASE) >> 4) as usize + 1] } else { 0 };
        }
        if offset < RUNTIME {
            return match offset - OP {
                R_USBCMD => self.cmd,
                R_USBSTS => self.status,
                // 4 KiB pages
                R_PAGESIZE => 1,
                R_DNCTRL => self.dnctrl,
                R_CRCR => if self.crr { CRCR_CRR } else { 0 },
                R_CRCR_HIGH => 0,
                R_DCBAAP => self.dcbaap[0],
                R_DCBAAP_HIGH => self.dcbaap[1],
                R_CONFIG => self.config,
                _ => 0,
            };
        }
        if offset < DOORBELLS {
            return match offset - RUNTIME {
                0x00 => self.mfindex(),
                0x20 => self.iman,
                0x24 => self.imod,
                0x28 => self.erstsz,
                0x30 => self.erstba[0],
                0x34 => self.erstba[1],
                0x38 => self.erdp[0],
                0x3C => self.erdp[1],
                _ => 0,
            };
        }
        if offset >= XECP && offset < XECP + 16 * EXTENDED.len() as u32 {
            let capability = EXTENDED[((offset - XECP) >> 4) as usize];
            let word = ((offset & 0xF) >> 2) as usize;
            // (USB legacy support: its semaphores)
            if offset == XECP {
                return capability[0] | self.legsup;
            }
            return if word < capability.len() { capability[word] } else { 0 };
        }
        // (doorbells read 0)
        0
    }

    fn command_ring_from_crcr(&mut self) {
        if !self.crr {
            self.command_ring = Ring { dequeue: (self.crcr[1] as u64) << 32 | (self.crcr[0] & !0x3F) as u64, ccs: self.crcr[0] & CRCR_RCS };
        }
    }

    unsafe fn write_register(&mut self, offset: u32, bits: u32, mask: u32) {
        let merge = |old: u32| old & !mask | bits & mask;
        if offset >= PORTS_BASE && offset < PORTS_BASE + 0x10 * ROOT_PORTS as u32 {
            if offset & 0xF == 0 {
                let root = ((offset - PORTS_BASE) >> 4) as usize + 1;
                let value = merge(self.portsc[root]) & !PORT_CHANGES | bits & mask & PORT_CHANGES;
                self.write_port(root, value);
            }
            return;
        }
        if offset == XECP {
            // the OS taking the controller from the firmware gets it at once (no USB SMI)
            self.legsup = merge(self.legsup) & (LEGSUP_BIOS_OWNED | LEGSUP_OS_OWNED);
            if self.legsup & LEGSUP_OS_OWNED != 0 {
                self.legsup &= !LEGSUP_BIOS_OWNED;
            }
            return;
        }
        if offset >= DOORBELLS && offset < DOORBELLS + 4 * (MAX_SLOTS as u32 + 1) {
            self.doorbell(((offset - DOORBELLS) >> 2) as usize, merge(0));
            return;
        }
        match offset {
            o if o == OP + R_USBCMD => {
                let value = merge(self.cmd);
                if value & CMD_HCRST != 0 {
                    self.reset();
                    return;
                }
                if value & CMD_RS != 0 && self.status & STS_HCH != 0 {
                    self.status &= !STS_HCH;
                    self.mf_start = js::microtick();
                    self.next_ms = js::microtick() + 1.0;
                }
                else if value & CMD_RS == 0 && self.status & STS_HCH == 0 {
                    self.status |= STS_HCH;
                }
                // (save and restore state: nothing to save)
                if value & CMD_CSS != 0 {
                    self.status &= !STS_SRE;
                }
                self.cmd = value & CMD_MASK & !(CMD_CSS | CMD_CRS);
                self.update_irq();
            },
            o if o == OP + R_USBSTS => {
                self.status &= !(bits & mask & (STS_HSE | STS_EINT | STS_PCD | STS_SRE));
                self.update_irq();
            },
            o if o == OP + R_DNCTRL => self.dnctrl = merge(self.dnctrl) & 0xFFFF,
            o if o == OP + R_CRCR => {
                let value = merge(self.crcr[0]);
                if value & (CRCR_CS | CRCR_CA) != 0 && self.crr {
                    // stopped (or aborted: commands run at once, none is under way)
                    self.crr = false;
                    let d = self.command_ring.dequeue;
                    self.event(d as u32, (d >> 32) as u32, CC_COMMAND_RING_STOPPED << 24, EVENT_COMMAND << 10);
                    return;
                }
                self.crcr[0] = value & !0x3F | value & CRCR_RCS;
                self.command_ring_from_crcr();
            },
            o if o == OP + R_CRCR_HIGH => {
                self.crcr[1] = merge(self.crcr[1]);
                self.command_ring_from_crcr();
            },
            o if o == OP + R_DCBAAP => self.dcbaap[0] = merge(self.dcbaap[0]) & !0x3F,
            o if o == OP + R_DCBAAP_HIGH => self.dcbaap[1] = merge(self.dcbaap[1]),
            o if o == OP + R_CONFIG => self.config = merge(self.config) & 0x3FF,
            o if o == RUNTIME + 0x20 => {
                // IMAN: IP written 1 is cleared; IE
                let value = merge(self.iman);
                self.iman = self.iman & !(value & IMAN_IP) & !IMAN_IE | value & IMAN_IE;
                self.update_irq();
            },
            o if o == RUNTIME + 0x24 => self.imod = merge(self.imod),
            o if o == RUNTIME + 0x28 => self.erstsz = merge(self.erstsz) & 0xFFFF,
            o if o == RUNTIME + 0x30 => {
                self.erstba[0] = merge(self.erstba[0]) & !0x3F;
                self.event_ring_reset();
            },
            o if o == RUNTIME + 0x34 => {
                self.erstba[1] = merge(self.erstba[1]);
                self.event_ring_reset();
            },
            o if o == RUNTIME + 0x38 => {
                // ERDP: the driver's dequeue pointer; EHB written 1 is cleared, and events still to read interrupt again
                let value = merge(self.erdp[0]);
                self.erdp[0] = value & !ERDP_EHB | self.erdp[0] & ERDP_EHB & !(value & ERDP_EHB);
                if value & ERDP_EHB != 0 && self.event_ring.size != 0 && self.event_dequeue() != self.event_ring.enqueue as i64 {
                    self.interrupt();
                }
            },
            o if o == RUNTIME + 0x3C => self.erdp[1] = merge(self.erdp[1]),
            _ => {},
        }
    }

    unsafe fn update_irq(&mut self) {
        let level = self.iman & IMAN_IP != 0 && self.iman & IMAN_IE != 0 && self.cmd & CMD_INTE != 0;
        if level == self.irq_level {
            return;
        }
        self.irq_level = level;
        super::pci::set_irq(self.pci_id, level);
    }

    unsafe fn event_ring_reset(&mut self) {
        let base = (self.erstba[1] as u64) << 32 | self.erstba[0] as u64;
        self.event_ring = EventRing { start: 0, size: 0, enqueue: 0, pcs: 1 };
        if self.erstsz == 0 || base == 0 {
            return;
        }
        let Some(entry) = self.dma_read(base, 16) else { return };
        let w = |i: usize| u32::from_le_bytes([entry[4 * i], entry[4 * i + 1], entry[4 * i + 2], entry[4 * i + 3]]);
        let size = w(2) & 0xFFFF;
        if size < 16 || size > 4096 {
            return;
        }
        self.event_ring = EventRing { start: (w(1) as u64) << 32 | (w(0) & !0x3F) as u64, size, enqueue: 0, pcs: 1 };
    }

    fn event_dequeue(&self) -> i64 {
        let erdp = (self.erdp[1] as u64) << 32 | (self.erdp[0] & !0xF) as u64;
        (erdp as i64 - self.event_ring.start as i64).div_euclid(16)
    }

    /// an event TRB to the event ring, and an interrupt; a full ring drops it, the last free entry takes an Event
    /// Ring Full Error
    unsafe fn event(&mut self, mut parameter_low: u32, mut parameter_high: u32, mut status: u32, mut control: u32) {
        let size = self.event_ring.size;
        if size == 0 {
            return;
        }
        let dequeue = self.event_dequeue();
        if dequeue < 0 || dequeue >= size as i64 {
            self.status |= STS_HCE;
            return;
        }
        let enqueue = self.event_ring.enqueue;
        if ((enqueue + 1) % size) as i64 == dequeue {
            return;
        }
        if ((enqueue + 2) % size) as i64 == dequeue {
            parameter_low = 0;
            parameter_high = 0;
            status = CC_EVENT_RING_FULL << 24;
            control = EVENT_HOST << 10;
        }
        let mut trb = [0u8; 16];
        trb[0..4].copy_from_slice(&parameter_low.to_le_bytes());
        trb[4..8].copy_from_slice(&parameter_high.to_le_bytes());
        trb[8..12].copy_from_slice(&status.to_le_bytes());
        trb[12..16].copy_from_slice(&(control | self.event_ring.pcs).to_le_bytes());
        let at = self.event_ring.start + 16 * enqueue as u64;
        if !self.dma_write(&trb, at) {
            return;
        }
        self.event_ring.enqueue += 1;
        if self.event_ring.enqueue == size {
            self.event_ring.enqueue = 0;
            self.event_ring.pcs ^= 1;
        }
        self.interrupt();
    }

    unsafe fn interrupt(&mut self) {
        let pending = self.erdp[0] & ERDP_EHB != 0;
        self.erdp[0] |= ERDP_EHB;
        self.iman |= IMAN_IP;
        self.status |= STS_EINT;
        if pending || self.iman & IMAN_IE == 0 || self.cmd & CMD_INTE == 0 {
            return;
        }
        self.update_irq();
    }

    /// the next TRB of a ring (Link TRBs followed), or None where the driver has not put one yet
    unsafe fn fetch(&mut self, ring: &mut Ring) -> Option<Trb> {
        for _ in 0..LINK_LIMIT {
            let bytes = self.dma_read(ring.dequeue, 16)?;
            let w = |i: usize| u32::from_le_bytes([bytes[4 * i], bytes[4 * i + 1], bytes[4 * i + 2], bytes[4 * i + 3]]);
            let control = w(3);
            if control & TRB_CYCLE != ring.ccs {
                return None;
            }
            let ty = control >> 10 & 0x3F;
            let (low, high) = (w(0), w(1));
            if ty == TRB_LINK {
                ring.dequeue = (high as u64) << 32 | (low & !0xF) as u64;
                if control & TRB_TC != 0 {
                    ring.ccs ^= 1;
                }
                continue;
            }
            let trb = Trb { address: ring.dequeue, low, high, status: w(2), control, ty, ccs: ring.ccs };
            ring.dequeue += 16;
            return Some(trb);
        }
        None
    }

    unsafe fn doorbell(&mut self, target: usize, value: u32) {
        if self.status & STS_HCH != 0 {
            return;
        }
        if target == 0 {
            if value == 0 {
                self.run_commands();
            }
            return;
        }
        let dci = value & 0xFF;
        if target > MAX_SLOTS || !(1..=31).contains(&dci) {
            return;
        }
        let state = match self.slots[target].as_ref().and_then(|s| s.endpoints[dci as usize].as_ref()) {
            Some(ep) => ep.state,
            None => return,
        };
        // (a stopped endpoint runs again; a halted one waits for Reset Endpoint)
        if state == EP_STOPPED {
            self.set_endpoint_state(target, dci, EP_RUNNING);
        }
        if !self.active.contains(&(target, dci)) {
            self.active.push((target, dci));
        }
        self.budget = DOORBELL_BYTES;
        self.run_endpoint(target, dci);
    }

    unsafe fn run_commands(&mut self) {
        self.crr = true;
        for _ in 0..256 {
            let mut ring = self.command_ring;
            let Some(trb) = self.fetch(&mut ring) else {
                self.command_ring = ring;
                break;
            };
            self.command_ring = ring;
            let mut slot_id = (trb.control >> 24) as usize;
            let mut code = CC_SUCCESS;
            match trb.ty {
                CMD_ENABLE_SLOT => match (1..=MAX_SLOTS).find(|&i| self.slots[i].is_none()) {
                    Some(i) => {
                        slot_id = i;
                        self.slots[i] = Some(Slot {
                            port: -1,
                            root: 0,
                            context: 0,
                            state: SLOT_DEFAULT,
                            gone: false,
                            endpoints: vec![None; 32],
                        });
                    },
                    None => {
                        slot_id = 0;
                        code = CC_NO_SLOTS;
                    },
                },
                CMD_NOOP => slot_id = 0,
                CMD_GET_PORT_BANDWIDTH => {
                    // (80% of every port free, as QEMU)
                    let mut bytes = vec![80u8; ROOT_PORTS + 1];
                    bytes[0] = 0;
                    code = if self.dma_write(&bytes, (trb.high as u64) << 32 | trb.low as u64) { CC_SUCCESS } else { CC_TRB };
                    slot_id = 0;
                },
                CMD_DISABLE_SLOT | CMD_ADDRESS_DEVICE | CMD_CONFIGURE_ENDPOINT | CMD_EVALUATE_CONTEXT | CMD_RESET_ENDPOINT
                | CMD_STOP_ENDPOINT | CMD_SET_TR_DEQUEUE | CMD_RESET_DEVICE => {
                    code = if slot_id < 1 || slot_id > MAX_SLOTS {
                        CC_TRB
                    }
                    else if self.slots[slot_id].is_none() {
                        CC_SLOT_NOT_ENABLED
                    }
                    else {
                        self.slot_command(&trb, slot_id)
                    };
                },
                _ => code = CC_TRB,
            }
            self.event(trb.address as u32, (trb.address >> 32) as u32, code << 24, EVENT_COMMAND << 10 | (slot_id as u32) << 24);
        }
    }

    fn slot(&mut self, id: usize) -> &mut Slot { self.slots[id].as_mut().unwrap() }

    unsafe fn slot_command(&mut self, trb: &Trb, id: usize) -> u32 {
        let dci = trb.control >> 16 & 0x1F;
        let pointer = (trb.high as u64) << 32 | (trb.low & !0xF) as u64;
        match trb.ty {
            CMD_DISABLE_SLOT => {
                for i in 1..=31 {
                    if self.slot(id).endpoints[i].is_some() {
                        self.disable_endpoint(id, i as u32);
                    }
                }
                self.slots[id] = None;
                CC_SUCCESS
            },
            CMD_ADDRESS_DEVICE => self.address_device(id, pointer, trb.control & TRB_BSR != 0),
            CMD_CONFIGURE_ENDPOINT => self.configure_endpoints(id, pointer, trb.control & TRB_DC != 0),
            CMD_EVALUATE_CONTEXT => self.evaluate_context(id, pointer),
            CMD_RESET_ENDPOINT => {
                let Some(ep) = self.slot(id).endpoints[dci as usize].as_mut() else { return CC_EP_NOT_ENABLED };
                if ep.state != EP_HALTED {
                    return CC_CONTEXT_STATE;
                }
                ep.td = None;
                self.set_endpoint_state(id, dci, EP_STOPPED);
                CC_SUCCESS
            },
            CMD_STOP_ENDPOINT => {
                let Some(ep) = self.slot(id).endpoints[dci as usize].as_mut() else { return CC_EP_NOT_ENABLED };
                if let Some(td) = ep.td.take() {
                    // the TD under way: a Transfer Event, Stopped, at its TRB; the ring from there
                    let i = td.index.min(td.trbs.len() - 1);
                    let now = td.trbs[i];
                    let left = (now.status & 0x1FFFF) as i64 - *td.done.get(td.index).unwrap_or(&0) as i64;
                    ep.ring = Ring { dequeue: now.address, ccs: now.ccs };
                    self.event(
                        now.address as u32,
                        (now.address >> 32) as u32,
                        CC_STOPPED << 24 | left.max(0) as u32,
                        EVENT_TRANSFER << 10 | dci << 16 | (id as u32) << 24,
                    );
                }
                self.set_endpoint_state(id, dci, EP_STOPPED);
                CC_SUCCESS
            },
            CMD_SET_TR_DEQUEUE => {
                let Some(ep) = self.slot(id).endpoints[dci as usize].as_mut() else { return CC_EP_NOT_ENABLED };
                if ep.state != EP_STOPPED && ep.state != EP_HALTED {
                    return CC_CONTEXT_STATE;
                }
                ep.ring = Ring { dequeue: pointer, ccs: trb.low & 1 };
                ep.td = None;
                let state = ep.state;
                self.set_endpoint_state(id, dci, state);
                CC_SUCCESS
            },
            CMD_RESET_DEVICE => {
                if self.slot(id).state < SLOT_ADDRESSED {
                    return CC_CONTEXT_STATE;
                }
                for i in 2..=31 {
                    if self.slot(id).endpoints[i].is_some() {
                        self.disable_endpoint(id, i as u32);
                    }
                }
                if let Some(port) = self.slot_device(id) {
                    self.reset_pending |= 1 << port;
                }
                self.slot(id).state = SLOT_DEFAULT;
                self.write_slot_state(id, SLOT_DEFAULT, 0, 1);
                CC_SUCCESS
            },
            _ => CC_TRB,
        }
    }

    /// the physical port of the device a slot was addressed to, still in its port and enabled
    fn slot_device(&self, id: usize) -> Option<usize> {
        let slot = self.slots[id].as_ref()?;
        if slot.port < 0 || slot.gone {
            return None;
        }
        let port = slot.port as usize;
        if self.attached[port] && self.root_port(port) as u32 == slot.root && self.portsc[slot.root as usize] & PORT_PED != 0 {
            Some(port)
        }
        else {
            None
        }
    }

    unsafe fn read_words(&mut self, address: u64, count: usize) -> Option<Vec<u32>> {
        let bytes = self.dma_read(address, 4 * count as u32)?;
        Some((0..count).map(|i| u32::from_le_bytes([bytes[4 * i], bytes[4 * i + 1], bytes[4 * i + 2], bytes[4 * i + 3]])).collect())
    }

    unsafe fn address_device(&mut self, id: usize, input: u64, bsr: bool) -> u32 {
        let dcbaa = (self.dcbaap[1] as u64) << 32 | self.dcbaap[0] as u64;
        let (Some(control), Some(words), Some(entry)) =
            (self.read_words(input, 2), self.read_words(input + 32, 16), self.read_words(dcbaa + 8 * id as u64, 2))
        else {
            return CC_TRB;
        };
        if control[0] != 0 || control[1] & 3 != 3 {
            return CC_TRB;
        }
        let mut slot_context: Vec<u32> = words[0..4].to_vec();
        let ep0: Vec<u32> = words[8..13].to_vec();
        // the root port (no hubs: no route string)
        let root = (slot_context[1] >> 16 & 0xFF) as usize;
        if root < 1 || root > ROOT_PORTS || slot_context[0] & 0xFFFFF != 0 {
            return CC_PARAMETER;
        }
        let port = (root - 1) % XHCI_PORTS;
        if !self.attached[port] || self.root_port(port) != root || self.portsc[root] & PORT_PED == 0 {
            return CC_TRANSACTION;
        }
        let taken = self.slots.iter().enumerate().any(|(i, other)| match other {
            Some(o) => i != id && o.port == port as i32 && !o.gone && o.state >= SLOT_ADDRESSED,
            None => false,
        });
        if taken {
            return CC_TRB;
        }
        let context = (entry[1] as u64) << 32 | (entry[0] & !0x3F) as u64;
        {
            let slot = self.slot(id);
            slot.port = port as i32;
            slot.root = root as u32;
            slot.gone = false;
            slot.context = context;
        }
        self.reset_pending |= 1 << port;
        if !bsr {
            // SET_ADDRESS to the slot's ID
            SCRATCH = [0x00, 0x05, id as u8, 0, 0, 0, 0, 0];
            self.transaction(port, PID_SETUP, 0, &raw mut SCRATCH as u32, 8);
            self.transaction(port, PID_IN, 0, 0, 0);
        }
        let state = if bsr { SLOT_DEFAULT } else { SLOT_ADDRESSED };
        self.slot(id).state = state;
        slot_context[3] = state << 27 | if bsr { 0 } else { id as u32 };
        self.write_dwords(context, &slot_context);
        if self.slot(id).endpoints[1].is_some() {
            self.disable_endpoint(id, 1);
        }
        self.enable_endpoint(id, 1, ep0);
        CC_SUCCESS
    }

    unsafe fn configure_endpoints(&mut self, id: usize, input: u64, deconfigure: bool) -> u32 {
        if self.slot(id).state < SLOT_ADDRESSED {
            return CC_CONTEXT_STATE;
        }
        if deconfigure {
            for i in 2..=31 {
                if self.slot(id).endpoints[i].is_some() {
                    self.disable_endpoint(id, i as u32);
                }
            }
            self.slot(id).state = SLOT_ADDRESSED;
            self.write_slot_state(id, SLOT_ADDRESSED, id as u32, -1);
            return CC_SUCCESS;
        }
        let Some(words) = self.read_words(input, 8 * 33) else { return CC_TRB };
        let (drop, add) = (words[0], words[1]);
        if drop & 3 != 0 || add & 3 != 1 {
            return CC_TRB;
        }
        for i in 2..=31usize {
            if drop & 1 << i != 0 || add & 1 << i != 0 {
                self.disable_endpoint(id, i as u32);
            }
            if add & 1 << i != 0 {
                let context: Vec<u32> = (0..5).map(|k| words[8 * (i + 1) + k]).collect();
                let ty = context[1] >> 3 & 7;
                if ty == EP_ISOCH_OUT || ty == EP_ISOCH_IN || ty == 0 {
                    return CC_PARAMETER;
                }
                self.enable_endpoint(id, i as u32, context);
            }
        }
        // the slot: its context entries, configured
        let entries = (words[8] >> 27) as i32;
        let any = self.slot(id).endpoints.iter().enumerate().any(|(i, ep)| ep.is_some() && i > 1);
        let state = if any { SLOT_CONFIGURED } else { SLOT_ADDRESSED };
        self.slot(id).state = state;
        self.write_slot_state(id, state, id as u32, entries);
        CC_SUCCESS
    }

    unsafe fn evaluate_context(&mut self, id: usize, input: u64) -> u32 {
        let context = self.slot(id).context;
        let (Some(words), Some(mut out)) = (self.read_words(input, 24), self.read_words(context, 16)) else { return CC_TRB };
        if context == 0 {
            return CC_TRB;
        }
        let (drop, add) = (words[0], words[1]);
        if drop != 0 || add & !3 != 0 {
            return CC_TRB;
        }
        if add & 1 != 0 {
            out[1] = out[1] & !0xFFFF | words[8 + 1] & 0xFFFF;
            out[2] = out[2] & 0x3FFFFF | words[8 + 2] & !0x3FFFFF;
        }
        if add & 2 != 0 {
            let max_packet = words[16 + 1] >> 16;
            out[8 + 1] = out[8 + 1] & 0xFFFF | max_packet << 16;
            if let Some(ep) = self.slot(id).endpoints[1].as_mut() {
                ep.max_packet = max_packet;
            }
        }
        self.write_dwords(context, &out);
        CC_SUCCESS
    }

    unsafe fn enable_endpoint(&mut self, id: usize, dci: u32, mut context: Vec<u32>) {
        let ep = Ep {
            dci,
            ty: context[1] >> 3 & 7,
            max_packet: context[1] >> 16,
            interval: context[0] >> 16 & 0xFF,
            ring: Ring { dequeue: (context[3] as u64) << 32 | (context[2] & !0xF) as u64, ccs: context[2] & 1 },
            state: EP_RUNNING,
            td: None,
        };
        let at = self.slot(id).context + 32 * dci as u64;
        self.slot(id).endpoints[dci as usize] = Some(ep);
        context[0] = context[0] & !7 | EP_RUNNING;
        self.write_dwords(at, &context);
    }

    unsafe fn disable_endpoint(&mut self, id: usize, dci: u32) {
        if self.slot(id).endpoints[dci as usize].is_none() {
            return;
        }
        self.set_endpoint_state(id, dci, EP_DISABLED);
        self.slot(id).endpoints[dci as usize] = None;
        self.active.retain(|&k| k != (id, dci));
    }

    /// an endpoint's state and dequeue pointer into its output context
    unsafe fn set_endpoint_state(&mut self, id: usize, dci: u32, state: u32) {
        let context = self.slot(id).context;
        let ep = self.slot(id).endpoints[dci as usize].as_mut().unwrap();
        ep.state = state;
        let ring = ep.ring;
        if context == 0 {
            return;
        }
        let at = context + 32 * dci as u64;
        let Some(mut w) = self.read_words(at, 4) else { return };
        w[0] = w[0] & !7 | state;
        w[2] = ring.dequeue as u32 & !0xF | ring.ccs;
        w[3] = (ring.dequeue >> 32) as u32;
        self.write_dwords(at, &w);
    }

    unsafe fn write_slot_state(&mut self, id: usize, state: u32, address: u32, entries: i32) {
        let context = self.slot(id).context;
        if context == 0 {
            return;
        }
        let Some(mut w) = self.read_words(context, 4) else { return };
        if entries >= 0 {
            w[0] = w[0] & 0x7FFFFFF | (entries as u32) << 27;
        }
        w[3] = state << 27 | address & 0xFF;
        self.write_dwords(context, &w);
    }

    /// a transaction with the device in a physical port, its data at `ptr` (this module's memory)
    unsafe fn transaction(&mut self, port: usize, pid: i32, endpoint: i32, ptr: u32, length: i32) -> i32 {
        // (the tablet is the core's; the others are asked through the bridge xhci.js sets: one physical port, any
        // address, the data at a pointer into this module's memory)
        if let Some(result) = super::usb_tablet::route(1 << port, -1, pid, endpoint, ptr as *mut u8, length as usize, &mut self.reset_pending) {
            return result;
        }
        js::usb_transaction(1 << port, -1, pid, endpoint, ptr, length)
    }

    unsafe fn run_endpoint(&mut self, id: usize, dci: u32) {
        for _ in 0..256 {
            if self.budget <= 0 {
                return;
            }
            let running = matches!(self.slots[id].as_ref().and_then(|s| s.endpoints[dci as usize].as_ref()), Some(ep) if ep.state == EP_RUNNING);
            if !running {
                self.active.retain(|&k| k != (id, dci));
                return;
            }
            if self.slot(id).endpoints[dci as usize].as_ref().unwrap().td.is_none() {
                let td = self.fetch_td(id, dci);
                match td {
                    Some(td) => self.slot(id).endpoints[dci as usize].as_mut().unwrap().td = Some(td),
                    None => {
                        self.active.retain(|&k| k != (id, dci));
                        return;
                    },
                }
            }
            if !self.run_td(id, dci) {
                return;
            }
            if let Some(ep) = self.slots[id].as_mut().and_then(|s| s.endpoints[dci as usize].as_mut()) {
                ep.td = None;
            }
        }
    }

    unsafe fn fetch_td(&mut self, id: usize, dci: u32) -> Option<Td> {
        let ep = self.slot(id).endpoints[dci as usize].as_ref().unwrap();
        let mut ring = ep.ring;
        let control_ep = ep.ty == EP_CONTROL;
        let mut trbs = Vec::new();
        let mut status = false;
        while trbs.len() < TD_TRBS {
            let trb = self.fetch(&mut ring)?;
            trbs.push(trb);
            // (a control transfer: until its Status stage and what is chained to it)
            if trb.ty == TRB_STATUS {
                status = true;
            }
            if control_ep && !status {
                continue;
            }
            if trb.control & TRB_CH == 0 {
                break;
            }
        }
        let n = trbs.len();
        Some(Td { trbs, index: 0, done: vec![0; n], short: -1, error: 0, error_index: -1, after: ring })
    }

    /// a TD's transactions from where it was: true once it is over, false while the device NAKs
    unsafe fn run_td(&mut self, id: usize, dci: u32) -> bool {
        let device = self.slot_device(id);
        let endpoint = (dci >> 1) as i32;
        let (max_packet, dci_in) = {
            let ep = self.slot(id).endpoints[dci as usize].as_ref().unwrap();
            (if ep.max_packet != 0 { ep.max_packet } else { 8 }, dci & 1 != 0)
        };
        let mut td = self.slot(id).endpoints[dci as usize].as_mut().unwrap().td.take().unwrap();
        let mut finished = true;
        'trbs: while td.index < td.trbs.len() && td.error == 0 {
            let trb = td.trbs[td.index];
            let i = td.index;
            if trb.ty == TRB_EVENT_DATA || trb.ty == TRB_NOOP {
                td.index += 1;
                continue;
            }
            // (after a short packet the TD's data is over; a control transfer's Status stage still runs)
            if td.short >= 0 && (trb.ty == TRB_NORMAL || trb.ty == TRB_DATA) {
                td.index += 1;
                continue;
            }
            if trb.ty == TRB_ISOCH || trb.ty != TRB_NORMAL && trb.ty != TRB_SETUP && trb.ty != TRB_DATA && trb.ty != TRB_STATUS {
                td.error = CC_TRB;
                break;
            }
            let Some(port) = device else {
                td.error = CC_TRANSACTION;
                break;
            };
            if trb.ty == TRB_SETUP {
                // (immediate data: the request's eight bytes)
                SCRATCH[0..4].copy_from_slice(&trb.low.to_le_bytes());
                SCRATCH[4..8].copy_from_slice(&trb.high.to_le_bytes());
                let result = self.transaction(port, PID_SETUP, 0, &raw mut SCRATCH as u32, 8);
                if result == USB_NAK {
                    finished = false;
                    break;
                }
                if result == USB_STALL || result == USB_NODEV {
                    td.error = if result == USB_STALL { CC_STALL } else { CC_TRANSACTION };
                    break;
                }
                td.done[i] = 8;
                td.index += 1;
                continue;
            }
            if trb.ty == TRB_STATUS {
                // the status stage: no data, the other way from the data stage
                let pid = if trb.control & TRB_DIR_IN != 0 { PID_IN } else { PID_OUT };
                let result = self.transaction(port, pid, 0, 0, 0);
                if result == USB_NAK {
                    finished = false;
                    break;
                }
                if result == USB_STALL || result == USB_NODEV {
                    td.error = if result == USB_STALL { CC_STALL } else { CC_TRANSACTION };
                    break;
                }
                td.index += 1;
                continue;
            }
            let length = trb.status & 0x1FFFF;
            let into = if trb.ty == TRB_DATA { trb.control & TRB_DIR_IN != 0 } else { dci_in };
            let buffer = (trb.high as u64) << 32 | trb.low as u64;
            while td.done[i] < length {
                if self.budget <= 0 {
                    finished = false;
                    break 'trbs;
                }
                let n = max_packet.min(length - td.done[i]);
                let ptr = if trb.control & TRB_IDT != 0 && !into {
                    SCRATCH[0..4].copy_from_slice(&trb.low.to_le_bytes());
                    SCRATCH[4..8].copy_from_slice(&trb.high.to_le_bytes());
                    if td.done[i] + n > 8 {
                        td.error = CC_DATA_BUFFER;
                        break;
                    }
                    &raw mut SCRATCH as u32 + td.done[i]
                }
                else {
                    match guest_ptr(buffer + td.done[i] as u64, n) {
                        Some(p) => p,
                        None => {
                            self.status |= STS_HSE;
                            td.error = CC_DATA_BUFFER;
                            break;
                        },
                    }
                };
                let result = self.transaction(port, if into { PID_IN } else { PID_OUT }, endpoint, ptr, n as i32);
                if result == USB_NAK {
                    finished = false;
                    break 'trbs;
                }
                if result == USB_STALL || result == USB_NODEV {
                    td.error = if result == USB_STALL { CC_STALL } else { CC_TRANSACTION };
                    break;
                }
                if result == USB_BABBLE {
                    td.error = CC_BABBLE;
                    break;
                }
                let moved = result as u32;
                if into && moved > 0 {
                    crate::jit::jit_dirty_cache(buffer as u32 + td.done[i], buffer as u32 + td.done[i] + moved);
                }
                td.done[i] += moved;
                self.budget -= (moved as i32).max(64);
                if into && moved < n {
                    td.short = i as i32;
                    break;
                }
            }
            if td.error != 0 {
                break;
            }
            td.index += 1;
        }
        if !finished {
            self.slot(id).endpoints[dci as usize].as_mut().unwrap().td = Some(td);
            return false;
        }
        if td.error != 0 {
            td.error_index = td.index.min(td.trbs.len() - 1) as i32;
        }
        self.report(id, dci, &td);
        true
    }

    /// a TD over: its Transfer Events, the ring past it, or the endpoint halted at it
    unsafe fn report(&mut self, id: usize, dci: u32, td: &Td) {
        let (mut reported, mut short, mut edtla) = (false, false, 0u32);
        for (i, trb) in td.trbs.iter().enumerate() {
            let length = if trb.ty == TRB_SETUP { 8 } else { trb.status & 0x1FFFF };
            if trb.ty == TRB_STATUS {
                reported = false;
                short = false;
            }
            if i as i32 == td.short {
                short = true;
            }
            if trb.ty == TRB_NORMAL || trb.ty == TRB_DATA {
                edtla += td.done[i];
            }
            let error = td.error != 0 && i as i32 == td.error_index;
            if !reported && (trb.control & TRB_IOC != 0 || short && trb.control & TRB_ISP != 0 || error) {
                let code = if error { td.error } else if short { CC_SHORT_PACKET } else { CC_SUCCESS };
                if trb.ty == TRB_EVENT_DATA {
                    // (the Event Data TRB's parameter, the bytes transferred so far; ED)
                    self.event(trb.low, trb.high, code << 24 | edtla & 0xFFFFFF, EVENT_TRANSFER << 10 | 1 << 2 | dci << 16 | (id as u32) << 24);
                    edtla = 0;
                }
                else {
                    let left = if trb.ty == TRB_STATUS { 0 } else { length.saturating_sub(td.done[i]) };
                    self.event(trb.address as u32, (trb.address >> 32) as u32, code << 24 | left, EVENT_TRANSFER << 10 | dci << 16 | (id as u32) << 24);
                }
                reported = true;
                if error {
                    break;
                }
            }
            if trb.ty == TRB_SETUP {
                reported = false;
                short = false;
            }
            if error {
                break;
            }
        }
        if td.error != 0 {
            // halted: the ring at the TD's first TRB (the driver resets the endpoint and moves on)
            let first = td.trbs[0];
            self.slot(id).endpoints[dci as usize].as_mut().unwrap().ring = Ring { dequeue: first.address, ccs: first.ccs };
            self.set_endpoint_state(id, dci, EP_HALTED);
            return;
        }
        self.slot(id).endpoints[dci as usize].as_mut().unwrap().ring = td.after;
    }

    unsafe fn timer(&mut self, now: f64) -> f64 {
        if self.status & STS_HCH != 0 || self.active.is_empty() {
            return 100.0;
        }
        if now < self.next_ms {
            return self.next_ms - now;
        }
        self.next_ms = now + 1.0;
        if super::pci::command(self.pci_id) & 4 == 0 {
            return 1.0;
        }
        self.budget = MS_BYTES;
        for (id, dci) in self.active.clone() {
            let exists = self.slots.get(id).and_then(|s| s.as_ref()).map_or(false, |s| s.endpoints[dci as usize].is_some());
            if !exists {
                self.active.retain(|&k| k != (id, dci));
                continue;
            }
            self.run_endpoint(id, dci);
        }
        1.0
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the core's interface

#[no_mangle]
pub unsafe fn xhci_init(pci_id: i32) {
    let x = xhci();
    x.present = true;
    MMIO_BASE = -1;
    x.pci_id = pci_id as usize & 0xFF;
    x.mmio = -1;
    x.legsup = LEGSUP_BIOS_OWNED;
    x.reset();
}

/// where BAR0 is decoded now (-1: not)
#[no_mangle]
pub unsafe fn xhci_set_mmio(base: f64) {
    xhci().mmio = base as i64;
    MMIO_BASE = if xhci().present { base as i64 } else { -1 };
}

/// where the MMIO is (-1: nowhere; always so in vCPU workers, where nothing initialises the controller)
static mut MMIO_BASE: i64 = -1;

#[no_mangle]
pub unsafe fn xhci_reset(all: bool) {
    let x = xhci();
    if all {
        x.legsup = LEGSUP_BIOS_OWNED;
    }
    x.reset();
}

/// a device in a physical port (attached), its speed on its root port, whether it is a SuperSpeed one; plugged
/// (notify: the slots of the device before end, the port's status changes) or as a restored snapshot has it
#[no_mangle]
pub unsafe fn xhci_attach(port: i32, attached: bool, superspeed: bool, speed: i32, notify: bool) {
    let x = xhci();
    let p = port as usize % XHCI_PORTS;
    if notify && x.attached[p] {
        for slot in x.slots.iter_mut().flatten() {
            if slot.port == p as i32 {
                slot.gone = true;
            }
        }
    }
    x.attached[p] = attached;
    x.superspeed[p] = superspeed;
    x.speed[p] = speed;
    if notify {
        x.update_port(p, true);
    }
}

/// a register access from JavaScript (vCPU workers' accesses come this way)
#[no_mangle]
pub unsafe fn xhci_mmio_read(offset: u32, size: u32) -> u32 {
    let value = xhci().register(offset & !3);
    if size == 4 { value } else { value >> ((offset & 3) << 3) & if size == 1 { 0xFF } else { 0xFFFF } }
}

#[no_mangle]
pub unsafe fn xhci_mmio_write(offset: u32, size: u32, value: u32) {
    let shift = (offset & 3) << 3;
    let mask = if size == 4 { u32::MAX } else { (if size == 1 { 0xFF } else { 0xFFFF }) << shift };
    xhci().write_register(offset & !3, value << shift, mask);
}

#[no_mangle]
pub unsafe fn xhci_port_connected(port: i32) -> bool {
    let x = xhci();
    let p = port as usize % XHCI_PORTS;
    (x.portsc[USB3_FIRST + p] | x.portsc[USB2_FIRST + p]) & PORT_CCS != 0
}

/// the ports whose device a reset put to its default state (and forgets them)
#[no_mangle]
pub unsafe fn xhci_take_resets() -> i32 {
    let x = xhci();
    let r = x.reset_pending;
    x.reset_pending = 0;
    r as i32
}

#[no_mangle]
pub unsafe fn xhci_update_irq() { xhci().update_irq() }

#[inline]
pub unsafe fn serves(addr: u32) -> bool {
    MMIO_BASE >= 0 && (addr as i64) >= MMIO_BASE && (addr as i64) < MMIO_BASE + MMIO_SIZE as i64
}

pub unsafe fn read8(addr: u32) -> i32 {
    let x = xhci();
    let offset = (addr as i64 - x.mmio) as u32;
    (x.register(offset & !3) >> ((offset & 3) << 3) & 0xFF) as i32
}

pub unsafe fn read32(addr: u32) -> i32 {
    let x = xhci();
    let offset = (addr as i64 - x.mmio) as u32;
    x.register(offset & !3) as i32
}

pub unsafe fn write(addr: u32, size: u32, value: u32) {
    let x = xhci();
    let offset = (addr as i64 - x.mmio) as u32;
    let shift = (offset & 3) << 3;
    let mask = if size == 4 { u32::MAX } else { (if size == 1 { 0xFF } else { 0xFFFF }) << shift };
    x.write_register(offset & !3, value << shift, mask);
}

/// From the core's timer pass
pub unsafe fn timer(now: f64) -> f64 {
    #[allow(static_mut_refs)]
    match XHCI.as_mut() {
        Some(x) if x.present => x.timer(now),
        _ => 100.0,
    }
}

// snapshots: the state as numbers (xhci.js builds its arrays from them, and back)
static mut STATE: Vec<f64> = Vec::new();

#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn xhci_state_addr() -> u32 { STATE.as_ptr() as u32 }
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn xhci_state_len() -> u32 { STATE.len() as u32 }
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn xhci_state_resize(n: u32) -> u32 {
    STATE.clear();
    STATE.resize(n as usize, 0.0);
    STATE.as_ptr() as u32
}

/// the state into STATE, in the order of xhci.js's get_state (see there)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn xhci_save() {
    let x = xhci();
    let s = &mut STATE;
    s.clear();
    let now = js::microtick();
    for v in [
        x.cmd as f64,
        x.status as f64,
        x.dnctrl as f64,
        x.crcr[0] as f64,
        x.crcr[1] as f64,
        x.command_ring.dequeue as f64,
        x.command_ring.ccs as f64,
        x.crr as u32 as f64,
        x.dcbaap[0] as f64,
        x.dcbaap[1] as f64,
        x.config as f64,
        x.iman as f64,
        x.imod as f64,
        x.erstsz as f64,
        x.erstba[0] as f64,
        x.erstba[1] as f64,
        x.erdp[0] as f64,
        x.erdp[1] as f64,
        x.event_ring.start as f64,
        x.event_ring.size as f64,
        x.event_ring.enqueue as f64,
        x.event_ring.pcs as f64,
        now - x.mf_start,
        x.legsup as f64,
        x.irq_level as u32 as f64,
    ] {
        s.push(v);
    }
    for p in x.portsc.iter() {
        s.push(*p as f64);
    }
    for slot in x.slots.iter() {
        let Some(slot) = slot else {
            s.push(0.0);
            continue;
        };
        s.extend([1.0, slot.port as f64, slot.root as f64, slot.context as f64, slot.state as f64, slot.gone as u32 as f64]);
        for ep in slot.endpoints.iter() {
            let Some(ep) = ep else {
                s.push(0.0);
                continue;
            };
            s.extend([1.0, ep.dci as f64, ep.ty as f64, ep.max_packet as f64, ep.interval as f64, ep.ring.dequeue as f64, ep.ring.ccs as f64, ep.state as f64]);
            match &ep.td {
                None => s.push(0.0),
                Some(td) => {
                    s.push(td.trbs.len() as f64 + 1.0);
                    for t in td.trbs.iter() {
                        s.extend([t.address as f64, t.low as f64, t.high as f64, t.status as f64, t.control as f64, t.ccs as f64]);
                    }
                    s.push(td.index as f64);
                    for d in td.done.iter() {
                        s.push(*d as f64);
                    }
                    s.extend([td.short as f64, td.error as f64, td.error_index as f64, td.after.dequeue as f64, td.after.ccs as f64]);
                },
            }
        }
    }
    s.push(x.active.len() as f64);
    for (id, dci) in x.active.iter() {
        s.extend([*id as f64, *dci as f64]);
    }
}

/// the state from STATE (written by xhci.js in the order of xhci_save)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn xhci_load() {
    let x = xhci();
    let s = &STATE;
    let mut i = 0;
    let mut next = || {
        let v = s.get(i).copied().unwrap_or(0.0);
        i += 1;
        v
    };
    x.cmd = next() as u32;
    x.status = next() as u32;
    x.dnctrl = next() as u32;
    x.crcr = [next() as u32, next() as u32];
    x.command_ring = Ring { dequeue: next() as u64, ccs: next() as u32 };
    x.crr = next() != 0.0;
    x.dcbaap = [next() as u32, next() as u32];
    x.config = next() as u32;
    x.iman = next() as u32;
    x.imod = next() as u32;
    x.erstsz = next() as u32;
    x.erstba = [next() as u32, next() as u32];
    x.erdp = [next() as u32, next() as u32];
    x.event_ring = EventRing { start: next() as u64, size: next() as u32, enqueue: next() as u32, pcs: next() as u32 };
    let now = js::microtick();
    x.mf_start = now - next();
    x.next_ms = now;
    x.legsup = next() as u32;
    x.irq_level = next() != 0.0;
    super::pci::set_irq(x.pci_id, x.irq_level);
    for p in x.portsc.iter_mut() {
        *p = next() as u32;
    }
    for slot in x.slots.iter_mut() {
        if next() == 0.0 {
            *slot = None;
            continue;
        }
        let mut sl = Slot {
            port: next() as i32,
            root: next() as u32,
            context: next() as u64,
            state: next() as u32,
            gone: next() != 0.0,
            endpoints: vec![None; 32],
        };
        for ep in sl.endpoints.iter_mut() {
            if next() == 0.0 {
                continue;
            }
            let mut e = Ep {
                dci: next() as u32,
                ty: next() as u32,
                max_packet: next() as u32,
                interval: next() as u32,
                ring: Ring { dequeue: next() as u64, ccs: next() as u32 },
                state: next() as u32,
                td: None,
            };
            let n = next() as usize;
            if n > 0 {
                let count = n - 1;
                let mut trbs = Vec::with_capacity(count);
                for _ in 0..count {
                    let (address, low, high, status, control, ccs) = (next() as u64, next() as u32, next() as u32, next() as u32, next() as u32, next() as u32);
                    trbs.push(Trb { address, low, high, status, control, ty: control >> 10 & 0x3F, ccs });
                }
                let index = next() as usize;
                let done = (0..count).map(|_| next() as u32).collect();
                let (short, error, error_index) = (next() as i32, next() as u32, next() as i32);
                let after = Ring { dequeue: next() as u64, ccs: next() as u32 };
                e.td = Some(Td { trbs, index, done, short, error, error_index, after });
            }
            *ep = Some(e);
        }
        *slot = Some(sl);
    }
    let n = next() as usize;
    x.active = (0..n).map(|_| (next() as usize, next() as u32)).collect();
}
