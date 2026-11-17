// The VMware backdoor's pointer (src/vmware.js): GETVERSION and the absolute pointer (status, data, commands) with
// its packet queue, at port 0x5658 (32-bit reads with EAX = the magic). The clipboard and GETTIME are the host's and
// stay in vmware.js, which also turns the page's mouse events into packets here (vmware_* exports).

use crate::cpu::cpu::{js, EAX, EBX, ECX, EDX};
use crate::cpu::global_pointers::reg32;

const VMWARE_PORT: i32 = 0x5658;
const VMWARE_MAGIC: i32 = 0x564D5868;
const CMD_GETVERSION: i32 = 10;
const CMD_ABSPOINTER_DATA: i32 = 39;
const CMD_ABSPOINTER_STATUS: i32 = 40;
const CMD_ABSPOINTER_COMMAND: i32 = 41;
const ABSPOINTER_ENABLE: i32 = 0x45414552;
const ABSPOINTER_DISABLE: i32 = 0x000000F5;
const ABSPOINTER_RELATIVE: i32 = 0x4C455252;
const ABSPOINTER_ABSOLUTE: i32 = 0x53424152;
const READ_ID: i32 = 0x3442554A;
const RELATIVE_PACKET: i32 = 0x00010000;
const QUEUE_MAX: usize = 1024;

pub struct Mouse {
    present: bool,
    pub enabled: bool,
    pub absolute: bool,
    queue: std::collections::VecDeque<i32>,
    buttons: i32,
    last_x: i32,
    last_y: i32,
    tail_is_move: bool,
}

static mut MOUSE: Mouse = Mouse {
    present: false,
    enabled: false,
    absolute: false,
    queue: std::collections::VecDeque::new(),
    buttons: 0,
    last_x: -1,
    last_y: -1,
    tail_is_move: false,
};

#[allow(static_mut_refs)]
fn mouse() -> &'static mut Mouse { unsafe { &mut MOUSE } }

unsafe fn reg(r: i32) -> &'static mut i32 { &mut *reg32.offset(r as isize) }

impl Mouse {
    fn push_packet(&mut self, status: i32, x: i32, y: i32, wheel: i32, move_only: bool) {
        if self.queue.len() + 4 > QUEUE_MAX {
            self.enabled = false;
            self.queue.clear();
            return;
        }
        self.queue.extend([status, x, y, wheel]);
        self.tail_is_move = move_only;
    }

    /// The tail packet's status, when the tail is a move packet that a new move can merge into
    fn mergeable(&self, relative: bool) -> bool {
        let n = self.queue.len();
        self.tail_is_move && n >= 4 && (self.queue[n - 4] & RELATIVE_PACKET != 0) == relative
    }

    fn push_absolute(&mut self, wheel: i32, move_only: bool) {
        if !self.enabled || !self.absolute || self.last_x < 0 {
            return;
        }
        if move_only && self.mergeable(false) {
            let n = self.queue.len();
            self.queue[n - 3] = self.last_x;
            self.queue[n - 2] = self.last_y;
            return;
        }
        self.push_packet(self.buttons, self.last_x, self.last_y, wheel, move_only);
    }

    fn push_relative(&mut self, dx: i32, dy: i32, wheel: i32, move_only: bool) {
        if !self.enabled {
            return;
        }
        if move_only && self.mergeable(true) {
            let n = self.queue.len();
            self.queue[n - 3] += dx;
            self.queue[n - 2] += dy;
            return;
        }
        self.push_packet(self.buttons | RELATIVE_PACKET, dx, dy, wheel, move_only);
    }

    unsafe fn read32(&mut self) -> i32 {
        match *reg(ECX) & 0xFFFF {
            CMD_GETVERSION => {
                *reg(EBX) = VMWARE_MAGIC;
                6
            },
            CMD_ABSPOINTER_STATUS => {
                if self.enabled { self.queue.len() as i32 } else { 0xFFFF0000u32 as i32 }
            },
            CMD_ABSPOINTER_DATA => {
                let n = (*reg(EBX) as u32).min(4).min(self.queue.len() as u32) as usize;
                let mut v = [0i32; 4];
                for slot in v.iter_mut().take(n) {
                    *slot = self.queue.pop_front().unwrap_or(0);
                }
                *reg(EBX) = v[1];
                *reg(ECX) = v[2];
                *reg(EDX) = v[3];
                v[0]
            },
            _ => {
                // CMD_ABSPOINTER_COMMAND (serves lets nothing else here)
                match *reg(EBX) {
                    ABSPOINTER_ENABLE => {
                        self.enabled = true;
                        self.queue.clear();
                        self.tail_is_move = false;
                        self.queue.push_back(READ_ID);
                    },
                    ABSPOINTER_DISABLE => {
                        self.enabled = false;
                        self.absolute = false;
                        self.queue.clear();
                        js::vmware_absolute(false);
                    },
                    ABSPOINTER_ABSOLUTE => {
                        self.absolute = true;
                        js::vmware_absolute(true);
                    },
                    ABSPOINTER_RELATIVE => {
                        self.absolute = false;
                        js::vmware_absolute(false);
                    },
                    _ => {},
                }
                0
            },
        }
    }
}

/// The core answers this access itself: a 32-bit read with the magic and one of the pointer's commands
#[inline]
pub unsafe fn serves(port: i32) -> bool {
    port == VMWARE_PORT
        && mouse().present
        && !super::parallel::WORKER
        && *reg(EAX) == VMWARE_MAGIC
        && matches!(*reg(ECX) & 0xFFFF, CMD_GETVERSION | CMD_ABSPOINTER_DATA | CMD_ABSPOINTER_STATUS | CMD_ABSPOINTER_COMMAND)
}

/// Also for vmware.js (32-bit reads through JavaScript)
#[no_mangle]
pub unsafe fn vmware_read32() -> i32 { mouse().read32() }

#[no_mangle]
pub unsafe fn vmware_init() {
    let m = mouse();
    *m = Mouse {
        present: true,
        enabled: false,
        absolute: false,
        queue: std::collections::VecDeque::new(),
        buttons: 0,
        last_x: -1,
        last_y: -1,
        tail_is_move: false,
    };
}

/// The host pointer's position (0..0xFFFF); false when it did not move
#[no_mangle]
pub unsafe fn vmware_move(x: i32, y: i32) {
    let m = mouse();
    if x == m.last_x && y == m.last_y {
        return;
    }
    m.last_x = x;
    m.last_y = y;
    m.push_absolute(0, true);
}

/// Buttons: left 0x20, middle 0x08, right 0x10
#[no_mangle]
pub unsafe fn vmware_buttons(buttons: i32) { mouse().buttons = buttons; }

#[no_mangle]
pub unsafe fn vmware_push_absolute(wheel: i32, move_only: bool) { mouse().push_absolute(wheel, move_only) }

#[no_mangle]
pub unsafe fn vmware_push_relative(dx: i32, dy: i32, wheel: i32, move_only: bool) {
    mouse().push_relative(dx, dy, wheel, move_only)
}

/// 0: enabled, 1: absolute
#[no_mangle]
pub unsafe fn vmware_state(i: i32) -> bool {
    let m = mouse();
    if i == 0 { m.enabled } else { m.absolute }
}

#[no_mangle]
pub unsafe fn vmware_set_state(enabled: bool, absolute: bool) {
    let m = mouse();
    m.enabled = enabled;
    m.absolute = absolute;
}
