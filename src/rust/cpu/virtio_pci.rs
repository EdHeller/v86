// The virtio PCI transport of the x86 machine (i440FX), as src/virtio.js's VirtIO with I/O BARs: the common
// configuration (feature words, device status, the queues' sizes, addresses and enables), the notification and ISR
// registers, answered here from the I/O ports. Each queue's state is here too (the rings: virtio.rs). virtio.js keeps
// the device models (for now), their device-specific configuration and the snapshot format: its fields are accessors
// of the ones here (Fields), and what a device has to do is asked of JavaScript (js::virtio_event): a notification of
// a queue, a reset, DRIVER_OK; the device-specific configuration's accesses through js::virtio_config. The display
// adapters' functions (capabilities in a memory BAR) and Q35 run virtio.js's own transport.

use crate::cpu::cpu::js;

pub const DEVICES: usize = 16;
pub const QUEUES: usize = 16;

// the fields of a device (i32), as virtio.js's VirtIO
pub const F_DEV_FSEL: usize = 0;
pub const F_DRV_FSEL: usize = 1;
pub const F_DEV_FEAT: usize = 2; // 4 words
pub const F_DRV_FEAT: usize = 6; // 4 words
pub const F_FEATURES_OK: usize = 10;
pub const F_STATUS: usize = 11;
pub const F_CFG_CHANGED: usize = 12;
pub const F_CFG_GEN: usize = 13;
pub const F_QSEL: usize = 14;
pub const F_ISR: usize = 15;
pub const F_NUM_QUEUES: usize = 16;
pub const F_BDF: usize = 17;
pub const F_NOTIFY_LEN: usize = 18;
pub const F_DEVCFG_LEN: usize = 19;
pub const F_PRESENT: usize = 20;
pub const F_IRQ_LEVEL: usize = 21;
pub const DEVICE_FIELDS: usize = 32;
// a queue's fields, from DEVICE_FIELDS + QUEUE_FIELDS * i, as VirtQueue
pub const Q_SIZE: usize = 0;
pub const Q_SIZE_SUPPORTED: usize = 1;
pub const Q_ENABLED: usize = 2;
pub const Q_NOTIFY_OFFSET: usize = 3;
pub const Q_DESC: usize = 4;
pub const Q_AVAIL: usize = 5;
pub const Q_AVAIL_LAST: usize = 6;
pub const Q_USED: usize = 7;
pub const Q_STAGED: usize = 8;
pub const Q_FIX_WRAPPING: usize = 9;
pub const QUEUE_FIELDS: usize = 10;
pub const FIELDS: usize = DEVICE_FIELDS + QUEUE_FIELDS * QUEUES;

const STATUS_DRIVER_OK: i32 = 4;
const STATUS_FEATURES_OK: i32 = 8;
const STATUS_NEEDS_RESET: i32 = 64;
const STATUS_FAILED: i32 = 128;
const ISR_DEVICE_CFG: i32 = 2;

// what js::virtio_event tells
pub const EVENT_NOTIFY: i32 = 0;
pub const EVENT_RESET: i32 = 1;
pub const EVENT_DRIVER_OK: i32 = 2;

static mut STATE: [[i32; FIELDS]; DEVICES] = [[0; FIELDS]; DEVICES];
static mut USED: usize = 0;

/// the common configuration's fields: (offset, bytes)
const COMMON: [(u32, u32); 20] = [
    (0, 4), (4, 4), (8, 4), (12, 4), (16, 2), (18, 2), (20, 1), (21, 1), (22, 2), (24, 2), (26, 2), (28, 2), (30, 2),
    (32, 4), (36, 4), (40, 4), (44, 4), (48, 4), (52, 4), (56, 0),
];

#[allow(static_mut_refs)]
fn dev(d: usize) -> &'static mut [i32; FIELDS] { unsafe { &mut STATE[d] } }

#[no_mangle]
pub unsafe fn virtio_pci_new() -> i32 {
    if USED == DEVICES {
        return -1;
    }
    STATE[USED] = [0; FIELDS];
    USED += 1;
    USED as i32 - 1
}

#[no_mangle]
pub unsafe fn virtio_pci_fields(d: i32) -> u32 { &raw mut STATE[d as usize] as u32 }

/// the device is ready (its fields filled in): its ports are answered here from now on
#[no_mangle]
pub unsafe fn virtio_pci_activate(d: i32) { dev(d as usize)[F_PRESENT] = 1; }

#[no_mangle]
pub unsafe fn virtio_pci_raise_irq(d: i32, ty: i32) {
    let f = dev(d as usize);
    f[F_ISR] |= ty;
    f[F_IRQ_LEVEL] = 1;
    super::pci::set_irq(f[F_BDF] as usize, true);
}

#[no_mangle]
pub unsafe fn virtio_pci_lower_irq(d: i32) {
    let f = dev(d as usize);
    f[F_ISR] = 0;
    f[F_IRQ_LEVEL] = 0;
    super::pci::set_irq(f[F_BDF] as usize, false);
}

unsafe fn notify_config_changes(d: usize) {
    let f = dev(d);
    f[F_CFG_CHANGED] = 1;
    if f[F_STATUS] & STATUS_DRIVER_OK != 0 {
        virtio_pci_raise_irq(d as i32, ISR_DEVICE_CFG);
    }
}

fn q(d: usize, i: usize) -> &'static mut [i32] {
    let f = dev(d);
    &mut f[DEVICE_FIELDS + QUEUE_FIELDS * i..DEVICE_FIELDS + QUEUE_FIELDS * (i + 1)]
}

fn selected(d: usize) -> Option<usize> {
    let f = dev(d);
    let s = f[F_QSEL] as usize;
    if s < f[F_NUM_QUEUES] as usize { Some(s) } else { None }
}

/// The device and its capability (0 common, 1 notification, 2 ISR, 3 device-specific) whose I/O BAR has `port`, and
/// the offset in it
#[inline]
#[allow(static_mut_refs)]
pub unsafe fn find(port: i32) -> Option<(usize, usize, u32)> {
    for d in 0..USED {
        let f = &STATE[d];
        if f[F_PRESENT] == 0 {
            continue;
        }
        let bdf = f[F_BDF] as usize;
        for bar in 0..4 {
            let base = super::pci::io_bar(bdf, bar);
            if base < 0 {
                continue;
            }
            let len = match bar {
                0 => 56,
                1 => f[F_NOTIFY_LEN],
                2 => 1,
                _ => f[F_DEVCFG_LEN],
            };
            if port >= base && port < base + len.max(1) {
                return Some((d, bar, (port - base) as u32));
            }
        }
    }
    None
}

/// Whether a port is one of the transport's here (not in vCPU workers, not on Q35: virtio.js answers there)
#[inline]
pub unsafe fn serves(port: i32) -> bool {
    USED != 0 && !super::parallel::WORKER && !super::pci::is_q35() && find(port).is_some()
}

unsafe fn common_read(d: usize, field: usize) -> i32 {
    let f = dev(d);
    let sel = selected(d);
    let qf = |k: usize| sel.map_or(0, |s| q(d, s)[k]);
    match field {
        0 => f[F_DEV_FSEL],
        1 => if (f[F_DEV_FSEL] as u32) < 4 { f[F_DEV_FEAT + f[F_DEV_FSEL] as usize] } else { 0 },
        2 => f[F_DRV_FSEL],
        3 => if (f[F_DRV_FSEL] as u32) < 4 { f[F_DRV_FEAT + f[F_DRV_FSEL] as usize] } else { 0 },
        // (no MSI-X)
        4 => 0xFFFF,
        5 => f[F_NUM_QUEUES],
        6 => f[F_STATUS],
        7 => f[F_CFG_GEN],
        8 => f[F_QSEL],
        9 => qf(Q_SIZE),
        10 => 0xFFFF,
        11 => qf(Q_ENABLED),
        12 => qf(Q_NOTIFY_OFFSET),
        13 => qf(Q_DESC),
        15 => qf(Q_AVAIL),
        17 => qf(Q_USED),
        _ => 0,
    }
}

unsafe fn common_write(d: usize, field: usize, data: i32) {
    let f = dev(d);
    let sel = selected(d);
    match field {
        0 => f[F_DEV_FSEL] = data,
        2 => f[F_DRV_FSEL] = data,
        3 => {
            let s = f[F_DRV_FSEL] as u32;
            let supported = if s < 4 { f[F_DEV_FEAT + s as usize] } else { 0 };
            if s < 4 {
                // (only features the device has: is_feature_negotiated relies on it)
                f[F_DRV_FEAT + s as usize] = data & supported;
            }
            if data & !supported != 0 {
                f[F_FEATURES_OK] = 0;
            }
        },
        6 => {
            let mut data = data & 0xFF;
            if data == 0 {
                // (the device and its model: virtio.js's reset, through the device's on_reset)
                js::virtio_event(d as i32, EVENT_RESET, 0);
                return;
            }
            let _ = STATUS_FAILED;
            if data & !f[F_STATUS] & STATUS_DRIVER_OK != 0 && f[F_STATUS] & STATUS_NEEDS_RESET != 0 {
                // (NEEDS_RESET could not be told before DRIVER_OK)
                notify_config_changes(d);
            }
            if f[F_FEATURES_OK] == 0 {
                data &= !STATUS_FEATURES_OK;
            }
            let driver_ok = data & !f[F_STATUS] & STATUS_DRIVER_OK != 0;
            f[F_STATUS] = data;
            if driver_ok {
                js::virtio_event(d as i32, EVENT_DRIVER_OK, 0);
            }
        },
        8 => f[F_QSEL] = data & 0xFFFF,
        9 => {
            let Some(s) = sel else { return };
            let mut size = data & 0xFFFF;
            if size & (size - 1) != 0 {
                size = 1 << (32 - (size - 1).leading_zeros());
            }
            let qs = q(d, s);
            if size > qs[Q_SIZE_SUPPORTED] {
                size = qs[Q_SIZE_SUPPORTED];
            }
            qs[Q_SIZE] = size;
        },
        11 => {
            let Some(s) = sel else { return };
            let qs = q(d, s);
            if data & 0xFFFF == 1 && qs[Q_DESC] != 0 && qs[Q_AVAIL] != 0 && qs[Q_USED] != 0 {
                qs[Q_ENABLED] = 1;
            }
        },
        13 => if let Some(s) = sel { q(d, s)[Q_DESC] = data },
        15 => if let Some(s) = sel { q(d, s)[Q_AVAIL] = data },
        17 => if let Some(s) = sel { q(d, s)[Q_USED] = data },
        _ => {},
    }
}

/// A read of `size` bits at `offset` of capability `cap`; as virtio.js's shims: a narrower read takes its part of the
/// field, a wider one the whole field
unsafe fn read(d: usize, cap: usize, offset: u32, size: i32) -> i32 {
    let (start, bytes, value) = match cap {
        0 => {
            let Some(i) = COMMON.iter().position(|&(o, b)| offset >= o && offset < o + b) else { return mask(size) };
            (COMMON[i].0, COMMON[i].1, common_read(d, i))
        },
        // (notifications read 0xFFFF)
        1 => (offset & !1, 2, 0xFFFF),
        2 => {
            // ISR: reading it clears it and the interrupt
            let isr = dev(d)[F_ISR];
            virtio_pci_lower_irq(d as i32);
            (0, 1, isr)
        },
        _ => return js::virtio_config(d as i32, offset as i32, size, false, 0),
    };
    if (size / 8) as u32 >= bytes {
        return value;
    }
    (value >> ((offset - start) * 8)) & mask(size)
}

fn mask(size: i32) -> i32 { if size == 32 { -1 } else { (1 << size) - 1 } }

unsafe fn write(d: usize, cap: usize, offset: u32, size: i32, value: i32) {
    match cap {
        0 => {
            let Some(i) = COMMON.iter().position(|&(o, b)| offset == o && (size / 8) as u32 == b) else { return };
            common_write(d, i, value);
        },
        1 => {
            if offset & 1 == 0 && size == 16 {
                js::virtio_event(d as i32, EVENT_NOTIFY, value & 0xFFFF | (offset as i32 >> 1) << 16);
            }
        },
        2 => {},
        _ => {
            js::virtio_config(d as i32, offset as i32, size, true, value);
        },
    }
}

pub unsafe fn port_read(port: i32, size: i32) -> i32 {
    match find(port) {
        Some((d, cap, offset)) => read(d, cap, offset, size),
        None => mask(size),
    }
}

pub unsafe fn port_write(port: i32, size: i32, value: i32) {
    if let Some((d, cap, offset)) = find(port) {
        write(d, cap, offset, size, value)
    }
}

/// From a device model: the configuration changed (virtio.js's notify_config_changes)
#[no_mangle]
pub unsafe fn virtio_pci_config_changed(d: i32) { notify_config_changes(d as usize) }
