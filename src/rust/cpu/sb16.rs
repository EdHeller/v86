// Sound Blaster 16 (src/sb16.js): the DSP (reset, its commands, read and write buffers, single-cycle and
// auto-initialised DMA playback of 8- and 16-bit, signed and unsigned, mono and stereo samples, direct DAC writes),
// the mixer, the OPL FM ports (accepted and ignored, as before), the MPU-401's UART reset, the IRQ and DMA
// configuration. The speaker pulls (sb16_dac_request) and takes the samples converted here (sb16_dac_take); what the
// bus has to know (rates, volumes, the DAC on and off) goes through js::sb16_event. sb16.js keeps the bus wiring, the
// DMA unmask listener and the snapshot format (STATE's fields).

use crate::cpu::cpu::{device_lower_irq, device_raise_irq, js};

const DSP_COPYRIGHT: &[u8] = b"COPYRIGHT (C) CREATIVE TECHNOLOGY LTD, 1992.";
const DSP_NO_COMMAND: i32 = 0;
const DSP_BUFSIZE: usize = 64;
const DSP_DACSIZE: usize = 65536;
pub const SB_DMA_BUFSIZE: usize = 65536;
const SB_DMA_BLOCK_SAMPLES: i32 = 1024;
const SB_IRQ_8BIT: usize = 1;
const SB_IRQ_16BIT: usize = 2;

// what js::sb16_event tells the bus
pub const EV_RATE: i32 = 0;
pub const EV_DAC_ENABLE: i32 = 1;
pub const EV_DAC_DISABLE: i32 = 2;
pub const EV_VOLUME: i32 = 3; // a: source, b: channel, c: dB
pub const EV_GAIN_LEFT: i32 = 4;
pub const EV_GAIN_RIGHT: i32 = 5;
pub const EV_TREBLE_LEFT: i32 = 6;
pub const EV_TREBLE_RIGHT: i32 = 7;
pub const EV_BASS_RIGHT: i32 = 8;

// the bus's mixer sources and channels (src/const.js)
const MIXER_CHANNEL_BOTH: i32 = 2;
const MIXER_CHANNEL_LEFT: i32 = 0;
const MIXER_CHANNEL_RIGHT: i32 = 1;
const MIXER_SRC_MASTER: i32 = 0;
const MIXER_SRC_PCSPEAKER: i32 = 1;
const MIXER_SRC_DAC: i32 = 2;

/// lib.js's ByteQueue, overwrite included
struct ByteQueue {
    data: [u8; DSP_BUFSIZE],
    start: usize,
    end: usize,
    length: usize,
}
impl ByteQueue {
    const fn new() -> ByteQueue { ByteQueue { data: [0; DSP_BUFSIZE], start: 0, end: 0, length: 0 } }
    fn push(&mut self, item: i32) {
        if self.length != DSP_BUFSIZE {
            self.length += 1;
        }
        self.data[self.end] = item as u8;
        self.end = (self.end + 1) & (DSP_BUFSIZE - 1);
    }
    fn shift(&mut self) -> i32 {
        if self.length == 0 {
            return -1;
        }
        let item = self.data[self.start];
        self.start = (self.start + 1) & (DSP_BUFSIZE - 1);
        self.length -= 1;
        item as i32
    }
    fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
        self.length = 0;
    }
}

/// lib.js's FloatQueue
struct FloatQueue {
    data: Vec<f32>,
    start: usize,
    end: usize,
    length: usize,
}
impl FloatQueue {
    fn new() -> FloatQueue { FloatQueue { data: vec![0.0; DSP_DACSIZE], start: 0, end: 0, length: 0 } }
    fn push(&mut self, item: f32) {
        if self.length == DSP_DACSIZE {
            self.start = (self.start + 1) & (DSP_DACSIZE - 1);
        }
        else {
            self.length += 1;
        }
        self.data[self.end] = item;
        self.end = (self.end + 1) & (DSP_DACSIZE - 1);
    }
    fn shift_all(&mut self, out: &mut Vec<f32>) {
        while self.length > 0 {
            out.push(self.data[self.start]);
            self.start = (self.start + 1) & (DSP_DACSIZE - 1);
            self.length -= 1;
        }
    }
    fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
        self.length = 0;
    }
}

/// The fields sb16.js's snapshots and accessors read (at these byte offsets), then the byte arrays
#[repr(C)]
pub struct State {
    sampling_rate: f64,                 // byte 0
    read_buffer_lastvalue: i32,         // byte 8
    command: i32,                       // byte 12
    command_size: i32,                  // byte 16
    mixer_current_address: i32,         // byte 20
    dummy_speaker_enabled: i32,         // byte 24
    test_register: i32,                 // byte 28
    dsp_highspeed: i32,                 // byte 32
    dsp_stereo: i32,                    // byte 36
    dsp_16bit: i32,                     // byte 40
    dsp_signed: i32,                    // byte 44
    dma_sample_count: i32,              // byte 48
    dma_bytes_count: i32,               // byte 52
    dma_bytes_left: i32,                // byte 56
    dma_bytes_block: i32,               // byte 60
    dma_irq: i32,                       // byte 64
    dma_channel: i32,                   // byte 68
    dma_channel_8bit: i32,              // byte 72
    dma_channel_16bit: i32,             // byte 76
    dma_autoinit: i32,                  // byte 80
    dma_waiting_transfer: i32,          // byte 84
    dma_paused: i32,                    // byte 88
    bytes_per_sample: i32,              // byte 92
    e2_value: i32,                      // byte 96
    e2_count: i32,                      // byte 100
    mpu_read_buffer_lastvalue: i32,     // byte 104
    irq: i32,                           // byte 108
    fm_current_address0: i32,           // byte 112
    fm_current_address1: i32,           // byte 116
    present: i32,                       // byte 120
    mixer_registers: [u8; 256],         // byte 124
    asp_registers: [u8; 256],           // byte 380
    irq_triggered: [u8; 16],            // byte 636
    dma_buffer: [u8; SB_DMA_BUFSIZE],   // byte 652
}

pub struct Sb16 {
    s: State,
    write_buffer: ByteQueue,
    read_buffer: ByteQueue,
    mpu_read_buffer: ByteQueue,
    dac: [FloatQueue; 2],
    taken: Vec<f32>,
}

static mut SB16: Option<Box<Sb16>> = None;

#[allow(static_mut_refs)]
fn sb() -> &'static mut Sb16 {
    unsafe {
        SB16.get_or_insert_with(|| {
            Box::new(Sb16 {
                s: std::mem::zeroed(),
                write_buffer: ByteQueue::new(),
                read_buffer: ByteQueue::new(),
                mpu_read_buffer: ByteQueue::new(),
                dac: [FloatQueue::new(), FloatQueue::new()],
                taken: Vec::new(),
            })
        })
    }
}

fn audio_normalize(value: f64, amplitude: f64, offset: f64) -> f32 { (value / amplitude + offset).clamp(-1.0, 1.0) as f32 }

impl Sb16 {
    unsafe fn event(&self, kind: i32, a: f64, b: i32, c: i32) { js::sb16_event(kind, a, b, c) }

    unsafe fn raise_irq(&mut self, ty: usize) {
        self.s.irq_triggered[ty] = 1;
        device_raise_irq(self.s.irq as u8);
    }
    unsafe fn lower_irq(&mut self, ty: usize) {
        self.s.irq_triggered[ty] = 0;
        device_lower_irq(self.s.irq as u8);
    }

    unsafe fn dsp_reset(&mut self) {
        self.write_buffer.clear();
        self.read_buffer.clear();
        let s = &mut self.s;
        s.command = DSP_NO_COMMAND;
        s.command_size = 0;
        s.dummy_speaker_enabled = 0;
        s.test_register = 0;
        s.dsp_highspeed = 0;
        s.dsp_stereo = 0;
        s.dsp_16bit = 0;
        s.dsp_signed = 0;
        self.dac[0].clear();
        self.dac[1].clear();
        s.dma_sample_count = 0;
        s.dma_bytes_count = 0;
        s.dma_bytes_left = 0;
        s.dma_bytes_block = 0;
        s.dma_irq = 0;
        s.dma_channel = 0;
        s.dma_autoinit = 0;
        s.dma_buffer.fill(0);
        s.dma_waiting_transfer = 0;
        s.dma_paused = 0;
        s.e2_value = 0xAA;
        s.e2_count = 0;
        s.sampling_rate = 22050.0;
        s.bytes_per_sample = 1;
        self.lower_irq(SB_IRQ_8BIT);
        self.s.irq_triggered.fill(0);
        self.s.asp_registers.fill(0);
        self.s.asp_registers[5] = 0x01;
        self.s.asp_registers[9] = 0xF8;
    }

    // mixer

    unsafe fn mixer_reset(&mut self) {
        let r = &mut self.s.mixer_registers;
        r[0x04] = 12 << 4 | 12;
        r[0x22] = 12 << 4 | 12;
        r[0x26] = 12 << 4 | 12;
        r[0x28] = 0;
        r[0x2E] = 0;
        r[0x0A] = 0;
        for i in 0x30..=0x35 {
            r[i] = 24 << 3;
        }
        for i in [0x36, 0x37, 0x38, 0x39, 0x3B, 0x3F, 0x40, 0x41, 0x42, 0x43] {
            r[i] = 0;
        }
        r[0x3C] = 0x1F;
        r[0x3D] = 0x15;
        r[0x3E] = 0x0B;
        for i in 0x44..=0x47 {
            r[i] = 8 << 4;
        }
        self.mixer_full_update();
    }

    unsafe fn mixer_full_update(&mut self) {
        for i in 1..256 {
            if legacy(i).is_some() {
                continue;
            }
            let v = self.s.mixer_registers[i];
            self.mixer_write(i, v as i32);
        }
    }

    unsafe fn mixer_read(&mut self, address: usize) -> i32 {
        if let Some((left, right)) = legacy(address) {
            return (self.s.mixer_registers[left] & 0xF0 | self.s.mixer_registers[right] >> 4) as i32;
        }
        match address {
            0x00 => {
                self.mixer_reset();
                0
            },
            0x80 => match self.s.irq {
                2 => 0x1,
                5 => 0x2,
                7 => 0x4,
                10 => 0x8,
                _ => 0,
            },
            0x81 => {
                let mut ret = 0;
                ret |= match self.s.dma_channel_8bit {
                    0 => 0x1,
                    1 => 0x2,
                    3 => 0x8,
                    _ => 0,
                };
                ret |= match self.s.dma_channel_16bit {
                    5 => 0x20,
                    6 => 0x40,
                    7 => 0x80,
                    _ => 0,
                };
                ret
            },
            0x82 => {
                let mut ret = 0x20;
                for i in 0..16 {
                    ret |= i * self.s.irq_triggered[i as usize] as i32;
                }
                ret
            },
            _ => self.s.mixer_registers[address] as i32,
        }
    }

    unsafe fn mixer_write(&mut self, address: usize, data: i32) {
        let data8 = data as u8;
        if let Some((left_reg, right_reg)) = legacy(address) {
            self.s.mixer_registers[address] = data8;
            let prev_left = self.s.mixer_registers[left_reg];
            let prev_right = self.s.mixer_registers[right_reg];
            let left = (data8 & 0xF0) | (prev_left & 0x0F);
            let right = ((data8 << 4) & 0xF0) | (prev_right & 0x0F);
            self.mixer_write(left_reg, left as i32);
            self.mixer_write(right_reg, right as i32);
            return;
        }
        let r = &mut self.s.mixer_registers;
        let volume = |src: i32, ch: i32, d: u8| (src, ch, (d >> 2) as i32 - 62);
        let ev: Option<(i32, f64, i32, i32)> = match address {
            0x00 => {
                r[0] = data8;
                None
            },
            0x30 | 0x31 | 0x32 | 0x33 => {
                r[address] = data8;
                let (src, ch) = match address {
                    0x30 => (MIXER_SRC_MASTER, MIXER_CHANNEL_LEFT),
                    0x31 => (MIXER_SRC_MASTER, MIXER_CHANNEL_RIGHT),
                    0x32 => (MIXER_SRC_DAC, MIXER_CHANNEL_LEFT),
                    _ => (MIXER_SRC_DAC, MIXER_CHANNEL_RIGHT),
                };
                let (a, b, c) = volume(src, ch, data8);
                Some((EV_VOLUME, a as f64, b, c))
            },
            0x3B => {
                r[0x3B] = data8;
                Some((EV_VOLUME, MIXER_SRC_PCSPEAKER as f64, MIXER_CHANNEL_BOTH, (data8 >> 6) as i32 * 6 - 18))
            },
            0x41 => {
                r[0x41] = data8;
                Some((EV_GAIN_LEFT, ((data8 >> 6) as i32 * 6) as f64, 0, 0))
            },
            0x42 => {
                r[0x42] = data8;
                Some((EV_GAIN_RIGHT, ((data8 >> 6) as i32 * 6) as f64, 0, 0))
            },
            0x44 | 0x45 | 0x46 | 0x47 => {
                r[address] = data8;
                let d = (data8 >> 3) as i32;
                let v = (d - if d < 16 { 14 } else { 16 }) as f64;
                // (as sb16.js: 0x46 and 0x47 both the right bass)
                let kind = match address {
                    0x44 => EV_TREBLE_LEFT,
                    0x45 => EV_TREBLE_RIGHT,
                    _ => EV_BASS_RIGHT,
                };
                Some((kind, v, 0, 0))
            },
            0x80 => {
                if data & 0x1 != 0 { self.s.irq = 2 }
                if data & 0x2 != 0 { self.s.irq = 5 }
                if data & 0x4 != 0 { self.s.irq = 7 }
                if data & 0x8 != 0 { self.s.irq = 10 }
                None
            },
            0x81 => {
                if data & 0x1 != 0 { self.s.dma_channel_8bit = 0 }
                if data & 0x2 != 0 { self.s.dma_channel_8bit = 1 }
                if data & 0x8 != 0 { self.s.dma_channel_8bit = 3 }
                if data & 0x20 != 0 { self.s.dma_channel_16bit = 5 }
                if data & 0x40 != 0 { self.s.dma_channel_16bit = 6 }
                if data & 0x80 != 0 { self.s.dma_channel_16bit = 7 }
                None
            },
            // (no handler: the write is dropped, as in sb16.js)
            _ => None,
        };
        if let Some((kind, a, b, c)) = ev {
            self.event(kind, a, b, c);
        }
    }

    // DSP

    unsafe fn port2xc_write(&mut self, value: i32) {
        if self.s.command == DSP_NO_COMMAND {
            self.s.command = value;
            self.write_buffer.clear();
            self.s.command_size = command_size(value as u8) as i32;
        }
        else {
            self.write_buffer.push(value);
        }
        if self.write_buffer.length as i32 >= self.s.command_size {
            self.command_do();
        }
    }

    unsafe fn sampling_rate_change(&mut self, rate: f64) {
        self.s.sampling_rate = rate;
        self.event(EV_RATE, rate, 0, 0);
    }

    fn channel_count(&self) -> f64 { if self.s.dsp_stereo != 0 { 2.0 } else { 1.0 } }

    fn dma_transfer_size_set(&mut self) {
        let lo = self.write_buffer.shift();
        let hi = self.write_buffer.shift();
        self.s.dma_sample_count = 1 + lo + (hi << 8);
    }

    unsafe fn dma_transfer_start(&mut self) {
        let s = &mut self.s;
        s.bytes_per_sample = if s.dsp_16bit != 0 { 2 } else { 1 };
        s.dma_bytes_count = s.dma_sample_count * s.bytes_per_sample;
        s.dma_bytes_block = SB_DMA_BLOCK_SAMPLES * s.bytes_per_sample;
        let max_bytes_block = (s.dma_bytes_count >> 2 & !0x3).max(32);
        s.dma_bytes_block = max_bytes_block.min(s.dma_bytes_block);
        s.dma_waiting_transfer = 1;
        if !super::dma::masked(s.dma_channel as usize) {
            let channel = s.dma_channel;
            self.dma_on_unmask(channel);
        }
    }

    unsafe fn dma_on_unmask(&mut self, channel: i32) {
        if channel != self.s.dma_channel || self.s.dma_waiting_transfer == 0 {
            return;
        }
        self.s.dma_waiting_transfer = 0;
        self.s.dma_bytes_left = self.s.dma_bytes_count;
        self.s.dma_paused = 0;
        self.event(EV_DAC_ENABLE, 0.0, 0, 0);
    }

    unsafe fn dma_transfer_next(&mut self) {
        let size = self.s.dma_bytes_left.min(self.s.dma_bytes_block).max(0) as usize;
        let samples = size / self.s.bytes_per_sample.max(1) as usize;
        let channel = self.s.dma_channel as usize;
        if !super::dma::transfer_to_device(channel, &mut self.s.dma_buffer, 0, size) {
            return;
        }
        self.dma_to_dac(samples);
        self.s.dma_bytes_left -= size as i32;
        if self.s.dma_bytes_left == 0 {
            let irq = self.s.dma_irq as usize;
            self.raise_irq(irq);
            if self.s.dma_autoinit != 0 {
                self.s.dma_bytes_left = self.s.dma_bytes_count;
            }
        }
    }

    fn dma_to_dac(&mut self, sample_count: usize) {
        let s = &self.s;
        let amplitude = if s.dsp_16bit != 0 { 32767.5 } else { 127.5 };
        let offset = if s.dsp_signed != 0 { 0.0 } else { -1.0 };
        let repeats = if s.dsp_stereo != 0 { 1 } else { 2 };
        let b = &s.dma_buffer;
        let mut channel = 0;
        for i in 0..sample_count {
            let raw = if s.dsp_16bit != 0 {
                let w = u16::from_le_bytes([b[2 * i], b[2 * i + 1]]);
                if s.dsp_signed != 0 { w as i16 as f64 } else { w as f64 }
            }
            else if s.dsp_signed != 0 {
                b[i] as i8 as f64
            }
            else {
                b[i] as f64
            };
            let sample = audio_normalize(raw, amplitude, offset);
            for _ in 0..repeats {
                self.dac[channel].push(sample);
                channel ^= 1;
            }
        }
    }

    unsafe fn command_do(&mut self) {
        let command = self.s.command;
        match command {
            0x08 => {
                if self.write_buffer.shift() == 0x03 {
                    self.read_buffer.clear();
                    self.read_buffer.push(0xFF);
                }
            },
            0x0F => {
                let i = self.write_buffer.shift();
                self.read_buffer.clear();
                // (an empty buffer's -1: undefined in sb16.js, pushed as 0)
                let v = if i >= 0 { self.s.asp_registers[i as usize] as i32 } else { 0 };
                self.read_buffer.push(v);
            },
            0x10 => {
                let value = audio_normalize(self.write_buffer.shift() as f64, 127.5, -1.0);
                self.dac[0].push(value);
                self.dac[1].push(value);
                self.event(EV_DAC_ENABLE, 0.0, 0, 0);
            },
            0x14 | 0x15 => {
                self.s.dma_irq = SB_IRQ_8BIT as i32;
                self.s.dma_channel = self.s.dma_channel_8bit;
                self.s.dma_autoinit = 0;
                self.s.dsp_signed = 0;
                self.s.dsp_16bit = 0;
                self.s.dsp_highspeed = 0;
                self.dma_transfer_size_set();
                self.dma_transfer_start();
            },
            0x1C => {
                self.s.dma_irq = SB_IRQ_8BIT as i32;
                self.s.dma_channel = self.s.dma_channel_8bit;
                self.s.dma_autoinit = 1;
                self.s.dsp_signed = 0;
                self.s.dsp_16bit = 0;
                self.s.dsp_highspeed = 0;
                self.dma_transfer_start();
            },
            0x20 => {
                self.read_buffer.clear();
                self.read_buffer.push(0x7F);
            },
            0x40 => {
                let tc = self.write_buffer.shift();
                let rate = 1000000.0 / (256 - tc) as f64 / self.channel_count();
                self.sampling_rate_change(rate);
            },
            0x41 | 0x42 => {
                let hi = self.write_buffer.shift();
                let lo = self.write_buffer.shift();
                self.sampling_rate_change(((hi << 8) | lo) as f64);
            },
            0x48 => self.dma_transfer_size_set(),
            0x90 => {
                self.s.dma_irq = SB_IRQ_8BIT as i32;
                self.s.dma_channel = self.s.dma_channel_8bit;
                self.s.dma_autoinit = 1;
                self.s.dsp_signed = 0;
                self.s.dsp_highspeed = 1;
                self.s.dsp_16bit = 0;
                self.dma_transfer_start();
            },
            0xB0..=0xBF | 0xC0..=0xCF => {
                if command & (1 << 3) != 0 {
                    // (not a transfer command)
                }
                else {
                    let mode = self.write_buffer.shift();
                    let sixteen = command < 0xC0;
                    self.s.dma_irq = if sixteen { SB_IRQ_16BIT } else { SB_IRQ_8BIT } as i32;
                    self.s.dma_channel = if sixteen { self.s.dma_channel_16bit } else { self.s.dma_channel_8bit };
                    self.s.dma_autoinit = (command & (1 << 2) != 0) as i32;
                    self.s.dsp_signed = (mode & (1 << 4) != 0) as i32;
                    self.s.dsp_stereo = (mode & (1 << 5) != 0) as i32;
                    self.s.dsp_16bit = sixteen as i32;
                    self.dma_transfer_size_set();
                    self.dma_transfer_start();
                }
            },
            0xD0 | 0xD5 => {
                self.s.dma_paused = 1;
                self.event(EV_DAC_DISABLE, 0.0, 0, 0);
            },
            0xD1 => self.s.dummy_speaker_enabled = 1,
            0xD3 => self.s.dummy_speaker_enabled = 0,
            0xD4 | 0xD6 => {
                self.s.dma_paused = 0;
                self.event(EV_DAC_ENABLE, 0.0, 0, 0);
            },
            0xD8 => {
                self.read_buffer.clear();
                self.read_buffer.push(self.s.dummy_speaker_enabled * 0xFF);
            },
            0xD9 | 0xDA => self.s.dma_autoinit = 0,
            0xE0 => {
                let v = self.write_buffer.shift();
                self.read_buffer.clear();
                self.read_buffer.push(!v);
            },
            0xE1 => {
                self.read_buffer.clear();
                self.read_buffer.push(4);
                self.read_buffer.push(5);
            },
            0xE3 => {
                self.read_buffer.clear();
                for &c in DSP_COPYRIGHT {
                    self.read_buffer.push(c as i32);
                }
                self.read_buffer.push(0);
            },
            0xE4 => self.s.test_register = self.write_buffer.shift(),
            0xE8 => {
                self.read_buffer.clear();
                self.read_buffer.push(self.s.test_register);
            },
            0xF2 => self.raise_irq(SB_IRQ_8BIT),
            0xF3 => self.raise_irq(SB_IRQ_16BIT),
            0xF9 => {
                let input = self.write_buffer.shift();
                self.read_buffer.clear();
                let v = match input {
                    0x0E => 0xFF,
                    0x0F => 0x07,
                    0x37 => 0x38,
                    _ => 0,
                };
                self.read_buffer.push(v);
            },
            _ => {},
        }
        self.s.command = DSP_NO_COMMAND;
        self.s.command_size = 0;
        self.write_buffer.clear();
    }

    unsafe fn port_read(&mut self, port: i32) -> i32 {
        match port {
            0x224 => self.s.mixer_current_address,
            0x225 => {
                let a = self.s.mixer_current_address as usize & 0xFF;
                self.mixer_read(a)
            },
            0x22A => {
                if self.read_buffer.length != 0 {
                    self.s.read_buffer_lastvalue = self.read_buffer.shift();
                }
                self.s.read_buffer_lastvalue
            },
            0x22C => 0x7F,
            0x22E => {
                if self.s.irq_triggered[SB_IRQ_8BIT] != 0 {
                    self.lower_irq(SB_IRQ_8BIT);
                }
                let ready = self.read_buffer.length != 0 && self.s.dsp_highspeed == 0;
                (ready as i32) << 7 | 0x7F
            },
            0x22F => {
                self.lower_irq(SB_IRQ_16BIT);
                0
            },
            0x330 => {
                if self.mpu_read_buffer.length != 0 {
                    self.s.mpu_read_buffer_lastvalue = self.mpu_read_buffer.shift();
                }
                self.s.mpu_read_buffer_lastvalue
            },
            0x331 => 0x80 * (self.mpu_read_buffer.length == 0) as i32,
            _ => 0xFF,
        }
    }

    unsafe fn port_write(&mut self, port: i32, value: i32) {
        match port {
            0x220 | 0x388 => self.s.fm_current_address0 = 0,
            0x222 => self.s.fm_current_address1 = 0,
            0x224 => self.s.mixer_current_address = value,
            0x225 => {
                let a = self.s.mixer_current_address as usize & 0xFF;
                self.mixer_write(a, value);
            },
            0x226 => {
                if self.s.dsp_highspeed != 0 {
                    self.s.dsp_highspeed = 0;
                }
                else if value != 0 {
                    self.dsp_reset();
                }
                self.read_buffer.clear();
                self.read_buffer.push(0xAA);
            },
            0x22C => self.port2xc_write(value),
            0x331 => {
                if value == 0xFF {
                    self.mpu_read_buffer.clear();
                    self.mpu_read_buffer.push(0xFE);
                }
            },
            // (FM data, the rest: ignored, as in sb16.js)
            _ => {},
        }
    }
}

/// The legacy mixer registers: the newer pair (left, right) they stand for
fn legacy(address: usize) -> Option<(usize, usize)> {
    match address {
        0x04 => Some((0x32, 0x33)),
        0x22 => Some((0x30, 0x31)),
        0x26 => Some((0x34, 0x35)),
        0x28 => Some((0x36, 0x37)),
        0x2E => Some((0x38, 0x39)),
        _ => None,
    }
}

/// The parameter bytes of each DSP command
fn command_size(command: u8) -> u8 {
    match command {
        0x04 | 0x08 | 0x0F | 0x10 | 0x40 | 0xE0 | 0xE2 | 0xE4 | 0xF9 => 1,
        0x05 | 0x0E | 0x14 | 0x15 | 0x16 | 0x17 | 0x24 | 0x41 | 0x42 | 0x48 | 0x74 | 0x75 | 0x76 | 0x77 | 0x80 => 2,
        0xB0..=0xCF => 3,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the core's interface

#[no_mangle]
pub unsafe fn sb16_init() {
    let sb = sb();
    sb.s.present = 1;
    sb.s.read_buffer_lastvalue = 0;
    sb.s.dma_channel_8bit = 1;
    sb.s.dma_channel_16bit = 5;
    sb.s.irq = 5;
    sb.s.sampling_rate = 22050.0;
    sb.mixer_reset();
    sb.dsp_reset();
}

#[no_mangle]
pub unsafe fn sb16_state() -> u32 { &raw mut sb().s as u32 }

#[no_mangle]
pub unsafe fn sb16_mixer_reset() { sb().mixer_reset() }

/// After a snapshot's mixer registers: the bus told of them again
#[no_mangle]
pub unsafe fn sb16_mixer_full_update() { sb().mixer_full_update() }

/// A DMA channel was unmasked (dma.js's listeners)
#[no_mangle]
pub unsafe fn sb16_dma_unmask(channel: i32) { sb().dma_on_unmask(channel) }

/// The speaker wants samples: the next DMA block when playing, then what is converted (sb16_dac_take)
#[no_mangle]
pub unsafe fn sb16_dac_request() {
    let sb = sb();
    if sb.s.dma_bytes_left != 0 && sb.s.dma_paused == 0 {
        sb.dma_transfer_next();
    }
}

/// The converted samples, taken: how many per channel (left ones, then right ones, at sb16_dac_samples)
#[no_mangle]
pub unsafe fn sb16_dac_take() -> i32 {
    let sb = sb();
    if sb.dac[0].length == 0 {
        return 0;
    }
    let mut taken = Vec::with_capacity(sb.dac[0].length + sb.dac[1].length);
    sb.dac[0].shift_all(&mut taken);
    let n = taken.len();
    sb.dac[1].shift_all(&mut taken);
    sb.taken = taken;
    n as i32
}

#[no_mangle]
pub unsafe fn sb16_dac_samples() -> u32 { sb().taken.as_ptr() as u32 }

#[inline]
pub unsafe fn serves(port: i32) -> bool {
    #[allow(static_mut_refs)]
    let present = SB16.as_ref().map_or(false, |s| s.s.present != 0);
    present && !super::parallel::WORKER && ((0x220..=0x22F).contains(&port) || port == 0x388 || port == 0x389 || port == 0x330 || port == 0x331)
}

pub unsafe fn port_read(port: i32) -> i32 {
    let port = if port == 0x388 || port == 0x389 { port - 0x388 + 0x220 } else { port };
    sb().port_read(port)
}

pub unsafe fn port_write(port: i32, value: i32) {
    let port = if port == 0x389 { 0x221 } else { port };
    sb().port_write(port, value & 0xFF)
}

/// The ports when reached through JavaScript (src/io.js: a parallel vCPU's accesses)
#[no_mangle]
pub unsafe fn sb16_port_read(port: i32) -> i32 { port_read(port) }
#[no_mangle]
pub unsafe fn sb16_port_write(port: i32, value: i32) { port_write(port, value) }
