// Serial ports (16550 UART): COM1 to COM4. Was src/uart.js, which keeps the bus messages (output bytes, modem
// lines), the snapshot format and the ports when they are reached through JavaScript.
//
// Output does not leave the module byte by byte: it goes to a buffer that JavaScript empties after each slice of
// the main loop (and at once when the buffer is full).
// http://wiki.osdev.org/UART
// https://www.freebsd.org/doc/en/articles/serial-uart/

use crate::cpu::apic;
use crate::cpu::cpu::{device_lower_irq, device_raise_irq};
use std::collections::VecDeque;

const DLAB: u32 = 0x80;

const UART_IER_MSI: u32 = 0x08; /* Modem Status Changed int. */
const UART_IER_THRI: u32 = 0x02; /* Enable Transmitter holding register int. */
const UART_IER_RDI: u32 = 0x01; /* Enable receiver data interrupt */

const UART_IIR_MSI: u32 = 0x00; /* Modem status interrupt (Low priority) */
const UART_IIR_NO_INT: u32 = 0x01;
const UART_IIR_THRI: u32 = 0x02; /* Transmitter holding register empty */
const UART_IIR_RDI: u32 = 0x04; /* Receiver data interrupt */
const UART_IIR_CTI: u32 = 0x0c; /* Character timeout */

// Modem control register
const UART_MCR_LOOPBACK: u32 = 0x10;

const UART_LSR_DATA_READY: u32 = 0x1; // data available
const UART_LSR_TX_EMPTY: u32 = 0x20; // TX (THR) buffer is empty
const UART_LSR_TRANSMITTER_EMPTY: u32 = 0x40; // TX empty and line is idle

pub const OUTPUT_SIZE: usize = 4096;

// (the layout is read by src/uart.js: the registers for snapshots, the output buffer after each slice)
#[repr(C)]
pub struct Uart {
    ints: u32,             // 0
    baud_rate: u32,        // 4
    line_control: u32,     // 8
    // line status register
    lsr: u32,              // 12
    fifo_control: u32,     // 16
    // interrupts enable
    ier: u32,              // 20
    // interrupt identification register
    iir: u32,              // 24
    modem_control: u32,    // 28
    modem_status: u32,     // 32
    scratch_register: u32, // 36
    irq: u32,              // 40
    present: u32,          // 44
    /// bytes written by the guest and not yet taken by JavaScript
    output_length: u32,    // 48
    output: [u8; OUTPUT_SIZE], // 52
}

const fn new_uart() -> Uart {
    Uart {
        ints: 1 << UART_IIR_THRI,
        baud_rate: 0,
        line_control: 0,
        lsr: UART_LSR_TRANSMITTER_EMPTY | UART_LSR_TX_EMPTY,
        fifo_control: 0,
        ier: 0,
        iir: UART_IIR_NO_INT,
        modem_control: 0,
        modem_status: 0,
        scratch_register: 0,
        irq: 0,
        present: 0,
        output_length: 0,
        output: [0; OUTPUT_SIZE],
    }
}

static mut UARTS: [Uart; 4] = [new_uart(), new_uart(), new_uart(), new_uart()];
static mut INPUT: [VecDeque<u8>; 4] = [VecDeque::new(), VecDeque::new(), VecDeque::new(), VecDeque::new()];

#[allow(static_mut_refs)]
unsafe fn uart(com: usize) -> &'static mut Uart { &mut UARTS[com] }
#[allow(static_mut_refs)]
unsafe fn input(com: usize) -> &'static mut VecDeque<u8> { &mut INPUT[com] }

/// COM1 0x3F8 (IRQ 4), COM2 0x2F8 (IRQ 3), COM3 0x3E8 (IRQ 4), COM4 0x2E8 (IRQ 3)
#[inline]
fn com_of(port: i32) -> Option<usize> {
    match port & !7 {
        0x3F8 => Some(0),
        0x2F8 => Some(1),
        0x3E8 => Some(2),
        0x2E8 => Some(3),
        _ => None,
    }
}

#[no_mangle]
pub unsafe fn get_uart_addr(com: i32) -> u32 { uart(com as usize & 3) as *mut Uart as u32 }

#[no_mangle]
pub unsafe fn uart_init(com: i32) {
    let com = com as usize & 3;
    *uart(com) = new_uart();
    uart(com).irq = if com & 1 == 0 { 4 } else { 3 };
    uart(com).present = 1;
    input(com).clear();
}

impl Uart {
    unsafe fn check_interrupt(&mut self) {
        let pending = |line: u32| self.ints & 1 << line != 0;
        let iir = if pending(UART_IIR_CTI) && self.ier & UART_IER_RDI != 0 {
            UART_IIR_CTI
        }
        else if pending(UART_IIR_RDI) && self.ier & UART_IER_RDI != 0 {
            UART_IIR_RDI
        }
        else if pending(UART_IIR_THRI) && self.ier & UART_IER_THRI != 0 {
            UART_IIR_THRI
        }
        else if pending(UART_IIR_MSI) && self.ier & UART_IER_MSI != 0 {
            UART_IIR_MSI
        }
        else {
            UART_IIR_NO_INT
        };
        self.iir = iir;
        if iir == UART_IIR_NO_INT {
            device_lower_irq(self.irq as u8);
        }
        else {
            device_raise_irq(self.irq as u8);
        }
    }

    unsafe fn throw_interrupt(&mut self, line: u32) {
        self.ints |= 1 << line;
        self.check_interrupt();
    }

    unsafe fn clear_interrupt(&mut self, line: u32) {
        self.ints &= !(1 << line);
        self.check_interrupt();
    }

    unsafe fn data_received(&mut self, com: usize, data: u8) {
        input(com).push_back(data);
        self.lsr |= UART_LSR_DATA_READY;
        if self.fifo_control & 1 != 0 {
            self.throw_interrupt(UART_IIR_CTI);
        }
        else {
            self.throw_interrupt(UART_IIR_RDI);
        }
    }

    unsafe fn write_data(&mut self, com: usize, out_byte: u8) {
        if self.line_control & DLAB != 0 {
            self.baud_rate = self.baud_rate & !0xFF | out_byte as u32;
            return;
        }
        self.throw_interrupt(UART_IIR_THRI);
        if self.modem_control & UART_MCR_LOOPBACK != 0 {
            self.data_received(com, out_byte);
        }
        else if (self.output_length as usize) < OUTPUT_SIZE {
            self.output[self.output_length as usize] = out_byte;
            self.output_length += 1;
        }
    }

    unsafe fn read(&mut self, com: usize, reg: i32) -> i32 {
        match reg {
            0 => {
                if self.line_control & DLAB != 0 {
                    return (self.baud_rate & 0xFF) as i32;
                }
                let data = input(com).pop_front().unwrap_or(0);
                if input(com).is_empty() {
                    self.lsr &= !UART_LSR_DATA_READY;
                    self.clear_interrupt(UART_IIR_CTI);
                    self.clear_interrupt(UART_IIR_RDI);
                }
                data as i32
            },
            1 => {
                if self.line_control & DLAB != 0 {
                    (self.baud_rate >> 8) as i32
                }
                else {
                    (self.ier & 0xF) as i32
                }
            },
            2 => {
                let mut ret = self.iir & 0xF;
                if self.iir == UART_IIR_THRI {
                    self.clear_interrupt(UART_IIR_THRI);
                }
                if self.fifo_control & 1 != 0 {
                    ret |= 0xC0;
                }
                ret as i32
            },
            3 => self.line_control as i32,
            4 => self.modem_control as i32,
            5 => self.lsr as i32,
            6 => {
                // (loopback: CTS, DSR, RI and DCD are RTS, DTR, OUT1 and OUT2)
                let result = if self.modem_control & UART_MCR_LOOPBACK != 0 {
                    self.modem_status & 0x0F | (self.modem_control & 0x0F) << 4
                }
                else {
                    self.modem_status
                };
                // clear MSR delta bits and interrupt flag
                self.modem_status &= 0xF0;
                self.clear_interrupt(UART_IIR_MSI);
                result as i32
            },
            _ => self.scratch_register as i32,
        }
    }

    unsafe fn write(&mut self, com: usize, reg: i32, out_byte: u8) {
        let value = out_byte as u32;
        match reg {
            0 => self.write_data(com, out_byte),
            1 => {
                if self.line_control & DLAB != 0 {
                    self.baud_rate = self.baud_rate & 0xFF | value << 8;
                }
                else {
                    if self.ier & UART_IIR_THRI == 0 && value & UART_IIR_THRI != 0 {
                        // re-throw THRI if it was masked
                        self.throw_interrupt(UART_IIR_THRI);
                    }
                    self.ier = value & 0xF;
                    self.check_interrupt();
                }
            },
            2 => self.fifo_control = value,
            3 => self.line_control = value,
            4 => self.modem_control = value,
            5 => {}, // Factory test write
            6 => {},
            _ => self.scratch_register = value,
        }
    }
}

/// The eight ports of a UART, also for src/uart.js
#[no_mangle]
pub unsafe fn uart_port_read(port: i32) -> i32 {
    match com_of(port) {
        Some(com) => uart(com).read(com, port & 7),
        None => 0xFF,
    }
}

#[no_mangle]
pub unsafe fn uart_port_write(port: i32, value: i32) {
    if let Some(com) = com_of(port) {
        uart(com).write(com, port & 7, value as u8);
    }
}

/// A byte for the guest (the bus message serial<n>-input)
#[no_mangle]
pub unsafe fn uart_receive(com: i32, data: i32) {
    let com = com as usize & 3;
    uart(com).data_received(com, data as u8);
}

/// Set or clear bit "msr_bit" in the Modem Status Register (MSR).
///
/// The upper nibble of the MSR reflects the current status of the 4 control lines (CTS/DSR/RI/DCD), the lower
/// nibble (the delta bits) indicates which of these have changed since the last time the MSR was read. If any
/// delta bit is set at the end of this operation and interrupts are enabled, a Modem Status Register interrupt
/// is raised.
#[no_mangle]
pub unsafe fn uart_set_modem_status_bit(com: i32, msr_bit: i32, set_bit: bool) {
    let u = uart(com as usize & 3);
    let new_modem_status = if set_bit { u.modem_status | 1 << msr_bit } else { u.modem_status & !(1 << msr_bit) };
    let new_delta_bits = (u.modem_status ^ new_modem_status) >> 4;
    u.modem_status = new_modem_status | new_delta_bits;
    if u.modem_status & 0x0F != 0 {
        u.throw_interrupt(UART_IIR_MSI);
    }
}

/// Whether the core answers this port itself: a UART of a one-core machine. Through JavaScript go the modem
/// control register (its lines are bus messages) and a data byte when the output buffer is full (JavaScript
/// empties it first)
#[inline]
pub unsafe fn serves_read(port: i32) -> bool {
    match com_of(port) {
        Some(com) => uart(com).present != 0 && apic::CORE_COUNT == 1,
        None => false,
    }
}
#[inline]
pub unsafe fn serves_write(port: i32) -> bool {
    match com_of(port) {
        Some(com) => {
            let u = uart(com);
            u.present != 0
                && apic::CORE_COUNT == 1
                && port & 7 != 4
                && !(port & 7 == 0 && u.output_length as usize >= OUTPUT_SIZE)
        },
        None => false,
    }
}
