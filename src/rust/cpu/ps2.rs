// PS/2 controller (8042) with keyboard and mouse: ports 0x60 and 0x64, IRQ 1 and 12. Was src/ps2.js, which keeps
// the bus messages (key codes, mouse movement, clicks, the wheel; "mouse-enable"), the reboot the controller can
// ask for, the timer that sends the rest of a large mouse movement and the snapshot format.

use crate::cpu::cpu::{device_lower_irq, device_raise_irq, FLAG_INTERRUPT};
use crate::cpu::global_pointers::flags;

const QUEUE_SIZE: usize = 1024;

/// (as lib.js's ByteQueue: a full queue overwrites)
struct ByteQueue {
    data: [u8; QUEUE_SIZE],
    start: usize,
    end: usize,
    length: usize,
}

impl ByteQueue {
    const fn new() -> ByteQueue { ByteQueue { data: [0; QUEUE_SIZE], start: 0, end: 0, length: 0 } }
    fn push(&mut self, item: i32) {
        if self.length != QUEUE_SIZE {
            self.length += 1;
        }
        self.data[self.end] = item as u8;
        self.end = (self.end + 1) & (QUEUE_SIZE - 1);
    }
    fn shift(&mut self) -> i32 {
        if self.length == 0 {
            return -1;
        }
        let item = self.data[self.start];
        self.start = (self.start + 1) & (QUEUE_SIZE - 1);
        self.length -= 1;
        item as i32
    }
    fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
        self.length = 0;
    }
}

// (the layout is read by src/ps2.js: two doubles, then 32-bit fields in this order)
#[repr(C)]
pub struct Ps2 {
    mouse_delta_x: f64,                // 0
    mouse_delta_y: f64,                // 8
    enable_mouse_stream: i32,          // 16: field 0
    use_mouse: i32,                    // 1
    have_mouse: i32,                   // 2
    mouse_clicks: i32,                 // 3
    have_keyboard: i32,                // 4
    enable_keyboard_stream: i32,       // 5
    next_is_mouse_command: i32,        // 6
    next_read_sample: i32,             // 7
    next_read_led: i32,                // 8
    next_handle_scan_code_set: i32,    // 9
    next_read_rate: i32,               // 10
    next_read_resolution: i32,         // 11
    last_port60_byte: i32,             // 12
    sample_rate: i32,                  // 13
    resolution: i32,                   // 14
    scaling2: i32,                     // 15
    command_register: i32,             // 16
    read_output_register: i32,         // 17
    read_command_register: i32,        // 18
    controller_output_port: i32,       // 19
    read_controller_output_port: i32,  // 20
    mouse_id: i32,                     // 21
    mouse_detect_state: i32,           // 22
    mouse_reset_workaround: i32,       // 23
    /// Also known as DBBOUT OBF - Output Buffer Full flag
    next_byte_is_ready: i32, // 24
    next_byte_is_aux: i32,             // 25
    last_kbd_code: i32,                // 26
    wheel_movement: i32,               // 27
    /// TempleOS's debugger polls the status port with interrupts off and reads mouse bytes as keys
    hide_mouse_while_cli: i32, // 28
    /// set when the guest turned the mouse on: JavaScript sends "mouse-enable" and clears it
    mouse_enable_event: i32, // 29
    present: i32,                      // 30
}

static mut PS2: Ps2 = new_ps2();
static mut KBD_BUFFER: ByteQueue = ByteQueue::new();
static mut MOUSE_BUFFER: ByteQueue = ByteQueue::new();

const fn new_ps2() -> Ps2 {
    Ps2 {
        mouse_delta_x: 0.0,
        mouse_delta_y: 0.0,
        enable_mouse_stream: 0,
        use_mouse: 0,
        have_mouse: 1,
        mouse_clicks: 0,
        have_keyboard: 1,
        enable_keyboard_stream: 0,
        next_is_mouse_command: 0,
        next_read_sample: 0,
        next_read_led: 0,
        next_handle_scan_code_set: 0,
        next_read_rate: 0,
        next_read_resolution: 0,
        last_port60_byte: 0,
        sample_rate: 100,
        resolution: 4,
        scaling2: 0,
        command_register: 1 | 4,
        read_output_register: 0,
        read_command_register: 0,
        // TODO: What should be the initial value?
        controller_output_port: 0,
        read_controller_output_port: 0,
        mouse_id: 0,
        mouse_detect_state: 0,
        mouse_reset_workaround: 0,
        next_byte_is_ready: 0,
        next_byte_is_aux: 0,
        last_kbd_code: 0,
        wheel_movement: 0,
        hide_mouse_while_cli: 0,
        mouse_enable_event: 0,
        present: 0,
    }
}

#[allow(static_mut_refs)]
fn ps2() -> &'static mut Ps2 { unsafe { &mut PS2 } }
#[allow(static_mut_refs)]
fn kbd() -> &'static mut ByteQueue { unsafe { &mut KBD_BUFFER } }
#[allow(static_mut_refs)]
fn mouse() -> &'static mut ByteQueue { unsafe { &mut MOUSE_BUFFER } }

#[no_mangle]
pub fn get_ps2_addr() -> u32 { &raw mut PS2 as u32 }

#[no_mangle]
pub unsafe fn ps2_init(hide_mouse_while_cli: bool) {
    *ps2() = new_ps2();
    ps2().hide_mouse_while_cli = hide_mouse_while_cli as i32;
    ps2().present = 1;
    kbd().clear();
    mouse().clear();
}

/// After a snapshot was restored into the fields
#[no_mangle]
pub unsafe fn ps2_state_restored() {
    ps2().next_byte_is_ready = 0;
    ps2().next_byte_is_aux = 0;
    kbd().clear();
    mouse().clear();
}

impl Ps2 {
    fn mouse_stream_active(&self) -> bool { self.enable_mouse_stream != 0 && self.command_register & 0x20 == 0 }

    unsafe fn raise_irq(&mut self) {
        if self.next_byte_is_ready != 0 {
            // Wait until previous byte is read
            // http://halicery.com/Hardware/8042/8042_1503033_TXT.htm
            return;
        }
        // Kbd has priority over aux
        if kbd().length != 0 {
            self.kbd_irq();
        }
        else if mouse().length != 0 {
            self.mouse_irq();
        }
    }

    unsafe fn mouse_irq(&mut self) {
        self.next_byte_is_ready = 1;
        self.next_byte_is_aux = 1;
        if self.command_register & 2 != 0 {
            // Pulse the irq line
            // Note: can't lower immediately after rising, so lower before rising
            // http://www.os2museum.com/wp/ibm-ps2-model-50-keyboard-controller/
            device_lower_irq(12);
            device_raise_irq(12);
        }
    }

    unsafe fn kbd_irq(&mut self) {
        self.next_byte_is_ready = 1;
        self.next_byte_is_aux = 0;
        if self.command_register & 1 != 0 {
            device_lower_irq(1);
            device_raise_irq(1);
        }
    }

    unsafe fn send_mouse_packet(&mut self, dx: i32, dy: i32) {
        let info_byte = ((dy < 0) as i32) << 5 | ((dx < 0) as i32) << 4 | 1 << 3 | self.mouse_clicks;

        mouse().push(info_byte);
        mouse().push(dx);
        mouse().push(dy);

        if self.mouse_id == 0x04 {
            // (TODO: the 4th and 5th button)
            mouse().push(self.wheel_movement & 0x0F);
            self.wheel_movement = 0;
        }
        else if self.mouse_id == 0x03 {
            mouse().push(self.wheel_movement & 0xFF); // Byte 4 - Z Movement
            self.wheel_movement = 0;
        }

        self.raise_irq();
    }

    /// Sends the accumulated movement as packets. A packet can hold -255..255 per axis, and a guest that is slow
    /// to handle interrupts (an interpreter) must not get a backlog of stale packets that make the pointer lag
    /// behind, so at most a few packets are queued; the rest of the movement stays accumulated and is sent when
    /// the guest has caught up. Returns whether movement is left (JavaScript calls again in 10 ms)
    unsafe fn flush_mouse_delta(&mut self) -> bool {
        if !self.mouse_stream_active() {
            return false;
        }
        const MAX_QUEUED_BYTES: usize = 12;
        while mouse().length < MAX_QUEUED_BYTES {
            let change_x = (self.mouse_delta_x as i32).clamp(-255, 255);
            let change_y = (self.mouse_delta_y as i32).clamp(-255, 255);
            if change_x == 0 && change_y == 0 {
                return false;
            }
            self.mouse_delta_x -= change_x as f64;
            self.mouse_delta_y -= change_y as f64;
            self.send_mouse_packet(change_x, change_y);
        }
        true
    }

    unsafe fn port60_read(&mut self) -> i32 {
        self.next_byte_is_ready = 0;

        if kbd().length == 0 && mouse().length == 0 {
            // should not happen
            return self.last_port60_byte;
        }

        if self.next_byte_is_aux != 0 {
            device_lower_irq(12);
            self.last_port60_byte = mouse().shift();
        }
        else {
            device_lower_irq(1);
            self.last_port60_byte = kbd().shift();
        }

        if kbd().length != 0 || mouse().length != 0 {
            self.raise_irq();
        }

        self.last_port60_byte
    }

    unsafe fn port64_read(&mut self) -> i32 {
        // status port
        let mut status_byte = 0x10;

        // A guest that polls this port with interrupts disabled (TempleOS in its debugger) reads every
        // pending byte as a keystroke, including the bytes of mouse packets, and shows them as
        // garbage text. Mouse data only becomes visible while interrupts are enabled, which is the
        // only situation in which the guest's mouse irq handler could read it anyway.
        // Only with the option ps2_hide_mouse_while_cli (the TempleOS page): every other guest reads mouse bytes
        // with interrupts disabled too, in its IRQ 12 handler and in Linux's i8042 probe (the AUX loopback), and
        // hiding them made Linux decide that there is no mouse port, or fail to enable the mouse.
        let mut hidden_mouse_byte =
            self.hide_mouse_while_cli != 0 && self.next_byte_is_aux != 0 && *flags & FLAG_INTERRUPT == 0;

        if hidden_mouse_byte && kbd().length != 0 {
            // A key must not wait behind a mouse byte that the guest cannot see: let it take the
            // output buffer; the mouse byte stays queued and is delivered afterwards.
            device_lower_irq(12);
            self.kbd_irq();
            hidden_mouse_byte = false;
        }

        if self.next_byte_is_ready != 0 && !hidden_mouse_byte {
            status_byte |= 0x1;
        }
        if self.next_byte_is_aux != 0 && !hidden_mouse_byte {
            status_byte |= 0x20;
        }
        status_byte
    }

    unsafe fn port60_write(&mut self, write_byte: i32) {
        if self.read_command_register != 0 {
            self.command_register = write_byte;
            self.read_command_register = 0;
        }
        else if self.read_output_register != 0 {
            self.read_output_register = 0;
            mouse().clear();
            mouse().push(write_byte);
            self.mouse_irq();
        }
        else if self.next_read_sample != 0 {
            self.next_read_sample = 0;
            mouse().clear();
            mouse().push(0xFA);

            self.sample_rate = write_byte;

            match self.mouse_detect_state {
                -1 => {
                    if write_byte == 60 {
                        // Detect Windows NT and turn on workaround the bug
                        // 200->100->80->60
                        self.mouse_reset_workaround = 1;
                        self.mouse_detect_state = 0;
                    }
                    else {
                        self.mouse_reset_workaround = 0;
                        self.mouse_detect_state = (write_byte == 200) as i32;
                    }
                },
                0 => {
                    if write_byte == 200 {
                        self.mouse_detect_state = 1;
                    }
                },
                1 => {
                    self.mouse_detect_state = match write_byte {
                        100 => 2,
                        200 => 3,
                        _ => 0,
                    }
                },
                2 => {
                    // Host sends sample rate 200->100->80 to activate Intellimouse wheel
                    if write_byte == 80 {
                        self.mouse_id = 0x03;
                    }
                    self.mouse_detect_state = -1;
                },
                3 => {
                    // Host sends sample rate 200->200->80 to activate Intellimouse 4th, 5th buttons
                    if write_byte == 80 {
                        self.mouse_id = 0x04;
                    }
                    self.mouse_detect_state = -1;
                },
                _ => {},
            }

            if self.sample_rate == 0 {
                self.sample_rate = 100;
            }

            self.mouse_irq();
        }
        else if self.next_read_resolution != 0 {
            self.next_read_resolution = 0;
            mouse().clear();
            mouse().push(0xFA);
            self.resolution = if write_byte > 3 { 4 } else { 1 << write_byte };
            self.mouse_irq();
        }
        else if self.next_read_led != 0 {
            // nope
            self.next_read_led = 0;
            kbd().push(0xFA);
            self.kbd_irq();
        }
        else if self.next_handle_scan_code_set != 0 {
            self.next_handle_scan_code_set = 0;
            kbd().push(0xFA);
            self.kbd_irq();
            if write_byte == 0 {
                kbd().push(1);
            }
        }
        else if self.next_read_rate != 0 {
            // nope
            self.next_read_rate = 0;
            kbd().push(0xFA);
            self.kbd_irq();
        }
        else if self.next_is_mouse_command != 0 {
            self.next_is_mouse_command = 0;

            if self.have_mouse == 0 {
                return;
            }

            // send ack
            kbd().clear();
            mouse().clear();
            mouse().push(0xFA);

            match write_byte {
                // set scaling to 1:1
                0xE6 => self.scaling2 = 0,
                // set scaling to 2:1
                0xE7 => self.scaling2 = 1,
                // set mouse resolution
                0xE8 => self.next_read_resolution = 1,
                // status request - send one packet; request single packet
                0xE9 | 0xEB => self.send_mouse_packet(0, 0),
                0xF2 => {
                    //  MouseID Byte
                    mouse().push(self.mouse_id);
                    self.mouse_clicks = 0;
                    self.mouse_delta_x = 0.0;
                    self.mouse_delta_y = 0.0;
                    self.raise_irq();
                },
                // sample rate
                0xF3 => self.next_read_sample = 1,
                0xF4 => {
                    // enable streaming
                    self.enable_mouse_stream = 1;
                    self.use_mouse = 1;
                    self.mouse_enable_event = 1;
                    self.mouse_clicks = 0;
                    self.mouse_delta_x = 0.0;
                    self.mouse_delta_y = 0.0;
                },
                // disable streaming
                0xF5 => self.enable_mouse_stream = 0,
                0xF6 => {
                    // set defaults
                    self.enable_mouse_stream = 0;
                    self.sample_rate = 100;
                    self.scaling2 = 0;
                    self.resolution = 4;
                },
                0xFF => {
                    // reset, send completion code
                    mouse().push(0xAA);
                    mouse().push(0);

                    self.use_mouse = 1;
                    self.mouse_enable_event = 1;

                    self.enable_mouse_stream = 0;
                    self.sample_rate = 100;
                    self.scaling2 = 0;
                    self.resolution = 4;

                    if self.mouse_reset_workaround == 0 {
                        self.mouse_id = 0x00;
                    }

                    self.mouse_clicks = 0;
                    self.mouse_delta_x = 0.0;
                    self.mouse_delta_y = 0.0;
                },
                _ => {},
            }

            self.mouse_irq();
        }
        else if self.read_controller_output_port != 0 {
            self.read_controller_output_port = 0;
            self.controller_output_port = write_byte;
            // If we ever want to implement A20 masking, here is where
            // we should turn the masking off if the second bit is on
        }
        else {
            // send ack
            mouse().clear();
            kbd().clear();
            kbd().push(0xFA);

            match write_byte {
                0xED => self.next_read_led = 1,
                // get/set scan code set
                0xF0 => self.next_handle_scan_code_set = 1,
                0xF2 => {
                    // identify
                    kbd().push(0xAB);
                    kbd().push(0x83);
                },
                //  Set typematic rate and delay
                0xF3 => self.next_read_rate = 1,
                // enable scanning
                0xF4 => self.enable_keyboard_stream = 1,
                // disable scanning
                0xF5 => self.enable_keyboard_stream = 0,
                0xFF => {
                    kbd().clear();
                    kbd().push(0xFA);
                    kbd().push(0xAA);
                    kbd().push(0);
                },
                _ => {},
            }

            self.kbd_irq();
        }
    }

    unsafe fn port64_write(&mut self, write_byte: i32) {
        let answer = |ps2: &mut Ps2, byte: i32| {
            kbd().clear();
            mouse().clear();
            kbd().push(byte);
            ps2.kbd_irq();
        };
        match write_byte {
            0x20 => {
                let c = self.command_register;
                answer(self, c)
            },
            0x60 => self.read_command_register = 1,
            0xD0 => {
                // read controller output port
                let c = self.controller_output_port;
                answer(self, c)
            },
            0xD1 => self.read_controller_output_port = 1,
            0xD3 => self.read_output_register = 1,
            0xD4 => self.next_is_mouse_command = 1,
            // Disable second port
            0xA7 => self.command_register |= 0x20,
            // Enable second port
            0xA8 => self.command_register &= !0x20,
            // test second ps/2 port
            0xA9 => answer(self, 0),
            0xAA => answer(self, 0x55),
            // Test first PS/2 port
            0xAB => answer(self, 0),
            // Disable Keyboard
            0xAD => self.command_register |= 0x10,
            // Enable Keyboard
            0xAE => self.command_register &= !0x10,
            // (0xFE, the reboot, is JavaScript's)
            _ => {},
        }
    }
}

/// Ports 0x60 and 0x64, also for src/ps2.js
#[no_mangle]
pub unsafe fn ps2_port_read(port: i32) -> i32 {
    match port {
        0x60 => ps2().port60_read(),
        0x64 => ps2().port64_read(),
        _ => 0xFF,
    }
}

#[no_mangle]
pub unsafe fn ps2_port_write(port: i32, value: i32) {
    match port {
        0x60 => ps2().port60_write(value & 0xFF),
        0x64 => ps2().port64_write(value & 0xFF),
        _ => {},
    }
}

/// A scan code from the host's keyboard
#[no_mangle]
pub unsafe fn ps2_kbd_send_code(code: i32) {
    let p = ps2();
    if p.enable_keyboard_stream != 0 {
        // Auto-repeat of a held key is a stream of identical make codes. A guest that is slow to
        // read them (an interpreter) would work through the backlog long after the key was
        // released, so drop repeats while the guest has not read the earlier ones.
        if code == p.last_kbd_code && code & 0x80 == 0 && kbd().length >= 4 {
            return;
        }
        p.last_kbd_code = code;
        kbd().push(code);
        p.raise_irq();
    }
}

/// Movement of the host's mouse (can be fractions). Returns whether movement is left to send
#[no_mangle]
pub unsafe fn ps2_mouse_send_delta(delta_x: f64, delta_y: f64) -> bool {
    let p = ps2();
    if p.have_mouse == 0 || p.use_mouse == 0 {
        return false;
    }
    p.mouse_delta_x += delta_x;
    p.mouse_delta_y += delta_y;
    p.flush_mouse_delta()
}

#[no_mangle]
pub unsafe fn ps2_mouse_flush() -> bool { ps2().flush_mouse_delta() }

#[no_mangle]
pub unsafe fn ps2_mouse_send_click(left: i32, middle: i32, right: i32) {
    let p = ps2();
    if p.have_mouse == 0 || p.use_mouse == 0 {
        return;
    }
    p.mouse_clicks = left | right << 1 | middle << 2;
    if p.mouse_stream_active() {
        p.send_mouse_packet(0, 0);
    }
}

#[no_mangle]
pub unsafe fn ps2_mouse_wheel(delta_y: f64, delta_x: f64) {
    let p = ps2();
    if p.have_mouse == 0 || p.use_mouse == 0 || !p.mouse_stream_active() {
        return;
    }
    let movement = p.wheel_movement as f64 - delta_y - delta_x * 2.0; // X Wheel Movement
    p.wheel_movement = movement.clamp(-8.0, 7.0) as i32;
    p.send_mouse_packet(0, 0);
}

/// Whether the core answers this port itself (not in a vCPU worker: the machine's instance has the devices). Through JavaScript go the commands to the
/// mouse (two of them turn it on: a bus message) and the controller's reboot command
#[inline]
pub unsafe fn serves_read(port: i32) -> bool { (port == 0x60 || port == 0x64) && ps2().present != 0 && !super::parallel::WORKER }
#[inline]
pub unsafe fn serves_write(port: i32, value: i32) -> bool {
    match port {
        0x60 => ps2().present != 0 && !super::parallel::WORKER && ps2().next_is_mouse_command == 0,
        0x64 => ps2().present != 0 && !super::parallel::WORKER && value & 0xFF != 0xFE,
        _ => false,
    }
}
