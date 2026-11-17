// Intel High Definition Audio controller and its codec (src/audio_devices/hd_audio/controller.js, codec.js) on
// i440FX machines: the registers in BAR0, the CORB and RIRB, the immediate command, the interrupts, the output
// stream's buffer descriptor list played on the machine clock (LPIB, the DMA position buffer, completion
// interrupts) and the samples through the codec's DAC (stream tag, channel, amplifier, pin). What it plays goes to
// JavaScript (js::hda_event): the samples for the speaker and the stream's own bytes. The plugin keeps the PCI
// function (identity, BAR, power management bytes), the snapshot format and, on Q35, its own controller.

use crate::cpu::cpu::js;
use crate::cpu::global_pointers::memory_size;
use crate::cpu::memory::mem8;

const MMIO_SIZE: u32 = 0x4000;
const INPUT_STREAMS: usize = 0;
const OUTPUT_STREAMS: usize = 1;
const STREAMS: usize = INPUT_STREAMS + OUTPUT_STREAMS;
const GCAP: u32 = (OUTPUT_STREAMS << 12 | INPUT_STREAMS << 8 | 1) as u32;

const R_GCAP: u32 = 0x00;
const R_VMIN: u32 = 0x02;
const R_VMAJ: u32 = 0x03;
const R_OUTPAY: u32 = 0x04;
const R_INPAY: u32 = 0x06;
const R_GCTL: u32 = 0x08;
const R_WAKEEN: u32 = 0x0C;
const R_STATESTS: u32 = 0x0E;
const R_GSTS: u32 = 0x10;
const R_OUTSTRMPAY: u32 = 0x18;
const R_INSTRMPAY: u32 = 0x1A;
const R_INTCTL: u32 = 0x20;
const R_INTSTS: u32 = 0x24;
const R_WALCLK: u32 = 0x30;
const R_SSYNC_OLD: u32 = 0x34;
const R_SSYNC: u32 = 0x38;
const R_CORBLBASE: u32 = 0x40;
const R_CORBUBASE: u32 = 0x44;
const R_CORBWP: u32 = 0x48;
const R_CORBRP: u32 = 0x4A;
const R_CORBCTL: u32 = 0x4C;
const R_CORBSTS: u32 = 0x4D;
const R_CORBSIZE: u32 = 0x4E;
const R_RIRBLBASE: u32 = 0x50;
const R_RIRBUBASE: u32 = 0x54;
const R_RIRBWP: u32 = 0x58;
const R_RINTCNT: u32 = 0x5A;
const R_RIRBCTL: u32 = 0x5C;
const R_RIRBSTS: u32 = 0x5D;
const R_RIRBSIZE: u32 = 0x5E;
const R_ICOI: u32 = 0x60;
const R_ICII: u32 = 0x64;
const R_ICIS: u32 = 0x68;
const R_DPLBASE: u32 = 0x70;
const R_DPUBASE: u32 = 0x74;
const R_STREAMS: u32 = 0x80;
const STREAM_SIZE: u32 = 0x20;
const R_WALCLK_ALIAS: u32 = 0x2030;
const R_LPIB_ALIAS: u32 = 0x2084;

const SD_CTL: u32 = 0x00;
const SD_STS: u32 = 0x03;
const SD_LPIB: u32 = 0x04;
const SD_CBL: u32 = 0x08;
const SD_LVI: u32 = 0x0C;
const SD_FIFOW: u32 = 0x0E;
const SD_FIFOS: u32 = 0x10;
const SD_FMT: u32 = 0x12;
const SD_BDPL: u32 = 0x18;
const SD_BDPU: u32 = 0x1C;

const GCTL_CRST: u32 = 1 << 0;
const GCTL_UNSOL: u32 = 1 << 8;
const INT_GIE: u32 = 1 << 31;
const INT_CIE: u32 = 1 << 30;
const CORBRP_RST: u32 = 1 << 15;
const CORBCTL_CMEIE: u32 = 1 << 0;
const CORBCTL_RUN: u32 = 1 << 1;
const RIRBWP_RST: u32 = 1 << 15;
const RIRBCTL_RINTCTL: u32 = 1 << 0;
const RIRBCTL_DMAEN: u32 = 1 << 1;
const RIRBSTS_RINTFL: u32 = 1 << 0;
const RIRBSTS_OIS: u32 = 1 << 2;
const RING_SIZE: u32 = 0x42;
const ICIS_ICB: u32 = 1 << 0;
const ICIS_IRV: u32 = 1 << 1;
const DPLBASE_ENABLE: u32 = 1 << 0;

const SD_CTL_SRST: u32 = 1 << 0;
const SD_CTL_RUN: u32 = 1 << 1;
const SD_CTL_IOCE: u32 = 1 << 2;
const SD_CTL_FEIE: u32 = 1 << 3;
const SD_CTL_DEIE: u32 = 1 << 4;
const SD_STS_BCIS: u32 = 1 << 2;
const SD_STS_FIFOE: u32 = 1 << 3;
const SD_STS_DESE: u32 = 1 << 4;
const SD_STS_FIFORDY: u32 = 1 << 5;
const FIFO_SIZE: u32 = 0xFF;

const WALLCLK_PER_MS: f64 = 24000.0;
const MAX_PLAY_MS: f64 = 100.0;

// what js::hda_event tells the plugin
pub const EV_OUTPUT: i32 = 0; // a: frames (hda_output_* has them)
pub const EV_RATE: i32 = 1;
pub const EV_ENABLE: i32 = 2;
pub const EV_DISABLE: i32 = 3;

/// SDnFMT: rate, bytes per sample, channels
fn decode_format(fmt: u32) -> (f64, u32, u32) {
    let base = if fmt & 1 << 14 != 0 { 44100.0 } else { 48000.0 };
    let rate = base * ((fmt >> 11 & 7) + 1) as f64 / ((fmt >> 8 & 7) + 1) as f64;
    let bytes = [1, 2, 4, 4, 4, 4, 4, 4][(fmt >> 4 & 7) as usize];
    (rate, bytes, (fmt & 0xF) + 1)
}

// ---------------------------------------------------------------------------------------------------------------
// the codec (codec.js): a generic output codec, QEMU's hda-output

const VENDOR_ID: u32 = 0x1AF40012;
const REVISION_ID: u32 = 0x00100101;
const NID_ROOT: u32 = 0;
const NID_AFG: u32 = 1;
const NID_DAC: u32 = 2;
const NID_PIN: u32 = 3;
const PCM_FORMATS: u32 = 1 << 17 | 1 << 5 | 1 << 6;
const SUPFMT_PCM: u32 = 1;
const PINCAP_OUT: u32 = 1 << 4;
const AMP_STEPS: u32 = 0x4A;
const AMP_CAPS: u32 = 1 << 31 | 3 << 16 | AMP_STEPS << 8 | AMP_STEPS;
const PIN_CONFIG: u32 = 0x00004010;
const PINCTL_OUT_EN: u32 = 1 << 6;
const WCAP_STEREO: u32 = 1 << 0;
const WCAP_OUT_AMP: u32 = 1 << 2;
const WCAP_AMP_OVRD: u32 = 1 << 3;
const WCAP_FORMAT_OVRD: u32 = 1 << 4;
const WCAP_CONN_LIST: u32 = 1 << 8;
const WID_PIN: u32 = 4;
const WCAP_TYPE_SHIFT: u32 = 20;

#[derive(Clone, Copy)]
pub struct Codec {
    pub stream: u32,
    pub channel: u32,
    pub format: u32,
    pub gain: [u32; 2],
    pub mute: [bool; 2],
    pub pin_control: u32,
    pub pin_config: u32,
    pub eapd: u32,
    pub unsolicited: u32,
    pub power: [u32; 4],
    pub subsystem_id: u32,
    pub digital: u32,
}

impl Codec {
    const ADDRESS: u32 = 0;

    fn new() -> Codec {
        Codec {
            stream: 0,
            channel: 0,
            format: 0x0011,
            gain: [AMP_STEPS; 2],
            mute: [false; 2],
            pin_control: PINCTL_OUT_EN,
            pin_config: PIN_CONFIG,
            eapd: 0,
            unsolicited: 0,
            power: [0; 4],
            subsystem_id: VENDOR_ID,
            digital: 0,
        }
    }

    fn parameter(&self, nid: u32, id: u32) -> u32 {
        match (nid, id) {
            (NID_ROOT, 0x00) => VENDOR_ID,
            (NID_ROOT, 0x01) => self.subsystem_id,
            (NID_ROOT, 0x02) => REVISION_ID,
            (NID_ROOT, 0x04) => 1 << 16 | 1,
            (NID_AFG, 0x05) => 0x01,
            (NID_AFG, 0x01) => self.subsystem_id,
            (NID_AFG, 0x04) => 2 << 16 | 2,
            (NID_AFG, 0x08) => 0x00000808,
            (NID_AFG, 0x0A) => PCM_FORMATS,
            (NID_AFG, 0x0B) => SUPFMT_PCM,
            (NID_DAC, 0x09) => WCAP_FORMAT_OVRD | WCAP_AMP_OVRD | WCAP_OUT_AMP | WCAP_STEREO,
            (NID_DAC, 0x0A) => PCM_FORMATS,
            (NID_DAC, 0x0B) => SUPFMT_PCM,
            (NID_DAC, 0x12) => AMP_CAPS,
            (NID_PIN, 0x09) => WID_PIN << WCAP_TYPE_SHIFT | WCAP_CONN_LIST | WCAP_STEREO,
            (NID_PIN, 0x0C) => PINCAP_OUT,
            (NID_PIN, 0x0E) => 1,
            _ => 0,
        }
    }

    fn command(&mut self, command: u32) -> u32 {
        let nid = command >> 20 & 0x7F;
        if nid > NID_PIN {
            return 0;
        }
        if command >> 16 & 7 != 7 {
            return self.verb4(nid, command >> 16 & 0xF, command & 0xFFFF);
        }
        self.verb12(nid, command >> 8 & 0xFFF, command & 0xFF)
    }

    fn verb4(&mut self, nid: u32, verb: u32, payload: u32) -> u32 {
        if nid != NID_DAC {
            return 0;
        }
        match verb {
            0x2 => {
                self.format = payload;
                0
            },
            0xA => self.format,
            0x3 => {
                if payload & 1 << 15 != 0 {
                    for (bit, side) in [(13, 0), (12, 1)] {
                        if payload & 1 << bit != 0 {
                            self.gain[side] = (payload & 0x7F).min(AMP_STEPS);
                            self.mute[side] = payload & 0x80 != 0;
                        }
                    }
                }
                0
            },
            0xB => {
                if payload & 1 << 15 == 0 {
                    return 0;
                }
                let side = if payload & 1 << 13 != 0 { 0 } else { 1 };
                (if self.mute[side] { 0x80 } else { 0 }) | self.gain[side]
            },
            _ => 0,
        }
    }

    fn verb12(&mut self, nid: u32, verb: u32, payload: u32) -> u32 {
        match verb {
            0xF00 => self.parameter(nid, payload),
            0xF20 => self.subsystem_id,
            0xF02 => {
                if nid == NID_PIN && payload == 0 { NID_DAC } else { 0 }
            },
            0xF01 | 0x701 => 0,
            0xF05 => self.power[nid as usize] << 4 | self.power[nid as usize],
            0x705 => {
                self.power[nid as usize] = payload & 0xF;
                0
            },
            0xF06 => {
                if nid == NID_DAC { self.stream << 4 | self.channel } else { 0 }
            },
            0x706 => {
                if nid == NID_DAC {
                    self.stream = payload >> 4;
                    self.channel = payload & 0xF;
                }
                0
            },
            0xF07 => {
                if nid == NID_PIN { self.pin_control } else { 0 }
            },
            0x707 => {
                if nid == NID_PIN {
                    self.pin_control = payload;
                }
                0
            },
            0xF08 => self.unsolicited,
            0x708 => {
                self.unsolicited = payload;
                0
            },
            0xF09 => 0,
            0xF0C => self.eapd,
            0x70C => {
                self.eapd = payload;
                0
            },
            0xF0D => self.digital,
            0x70D => {
                self.digital = self.digital & !0xFF | payload;
                0
            },
            0xF1C => {
                if nid == NID_PIN { self.pin_config } else { 0 }
            },
            0xF2D => {
                if nid == NID_DAC { 1 } else { 0 }
            },
            0x72D => 0,
            0x7FF => {
                if nid == NID_AFG {
                    *self = Codec::new();
                }
                0
            },
            0x71C..=0x71F => {
                if nid == NID_PIN {
                    let shift = (verb - 0x71C) << 3;
                    self.pin_config = self.pin_config & !(0xFF << shift) | payload << shift;
                }
                0
            },
            0x720..=0x723 => {
                let shift = (verb - 0x720) << 3;
                self.subsystem_id = self.subsystem_id & !(0xFF << shift) | payload << shift;
                0
            },
            _ => 0,
        }
    }

    /// The DAC's gains for stream `tag` (linear; 0 when muted or the pin is off), None when it does not play it
    fn output_for(&self, tag: u32) -> Option<[f32; 2]> {
        if tag == 0 || self.stream != tag {
            return None;
        }
        let on = self.pin_control & PINCTL_OUT_EN != 0;
        let g = |side: usize| {
            if on && !self.mute[side] { 10f64.powf((self.gain[side] as f64 - AMP_STEPS as f64) / 20.0) as f32 } else { 0.0 }
        };
        Some([g(0), g(1)])
    }
}

// ---------------------------------------------------------------------------------------------------------------
// the controller

#[derive(Clone, Copy, Default)]
pub struct Stream {
    pub index: u32,
    pub ctl: u32,
    pub sts: u32,
    pub lpib: u32,
    pub cbl: u32,
    pub lvi: u32,
    pub fifow: u32,
    pub fmt: u32,
    pub bdpl: u32,
    pub bdpu: u32,
    pub entry: u32,
    pub offset: u32,
    pub started: f64,
    pub played: f64,
    pub running: bool,
}

impl Stream {
    fn new(index: u32) -> Stream { Stream { index, ..Default::default() } }
    fn output(&self) -> bool { self.index as usize >= INPUT_STREAMS }
}

pub struct Hda {
    present: bool,
    pci_id: usize,
    mmio: i64,
    pub gctl: u32,
    pub wakeen: u32,
    pub statests: u32,
    pub gsts: u32,
    pub intctl: u32,
    pub ssync: u32,
    pub corb_base: [u32; 2],
    pub corb_wp: u32,
    pub corb_rp: u32,
    pub corb_rp_reset: bool,
    pub corbctl: u32,
    pub corbsts: u32,
    pub rirb_base: [u32; 2],
    pub rirb_wp: u32,
    pub rintcnt: u32,
    pub rirbctl: u32,
    pub rirbsts: u32,
    pub rirb_count: u32,
    pub icoi: u32,
    pub icii: u32,
    pub icis: u32,
    pub dplbase: u32,
    pub dpubase: u32,
    pub irq_level: bool,
    pub speaker_rate: f64,
    pub wall_base: f64,
    pub streams: [Stream; STREAMS],
    pub codec: Codec,
    // the last output: left and right samples, the stream's bytes, and which stream (index, format, tag)
    out_left: Vec<f32>,
    out_right: Vec<f32>,
    out_pcm: Vec<u8>,
    out_info: [u32; 3],
}

static mut HDA: Option<Box<Hda>> = None;
static mut MMIO_BASE: i64 = -1;

#[allow(static_mut_refs)]
fn hda() -> &'static mut Hda {
    unsafe {
        HDA.get_or_insert_with(|| {
            Box::new(Hda {
                present: false,
                pci_id: 0,
                mmio: -1,
                gctl: 0,
                wakeen: 0,
                statests: 0,
                gsts: 0,
                intctl: 0,
                ssync: 0,
                corb_base: [0; 2],
                corb_wp: 0,
                corb_rp: 0,
                corb_rp_reset: false,
                corbctl: 0,
                corbsts: 0,
                rirb_base: [0; 2],
                rirb_wp: 0,
                rintcnt: 0,
                rirbctl: 0,
                rirbsts: 0,
                rirb_count: 0,
                icoi: 0,
                icii: 0,
                icis: 0,
                dplbase: 0,
                dpubase: 0,
                irq_level: false,
                speaker_rate: 0.0,
                wall_base: 0.0,
                streams: [Stream::new(0); STREAMS],
                codec: Codec::new(),
                out_left: Vec::new(),
                out_right: Vec::new(),
                out_pcm: Vec::new(),
                out_info: [0; 3],
            })
        })
    }
}

fn ring_address(base: [u32; 2]) -> u64 { (base[1] as u64) << 32 | base[0] as u64 }

/// guest memory for DMA (None outside it, as read_physical's exception)
unsafe fn guest(address: u64, length: u32) -> Option<*mut u8> {
    if address + length as u64 > *memory_size as u64 {
        return None;
    }
    Some(mem8.add(address as usize))
}

unsafe fn dma_read(address: u64, length: u32) -> Option<&'static [u8]> {
    guest(address, length).map(|p| std::slice::from_raw_parts(p, length as usize))
}

unsafe fn dma_read32(address: u64) -> Option<u32> {
    dma_read(address, 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

unsafe fn dma_write(bytes: &[u8], address: u64) {
    if let Some(p) = guest(address, bytes.len() as u32) {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        crate::jit::jit_dirty_cache(address as u32, address as u32 + bytes.len() as u32);
    }
}

/// The register a byte belongs to: its first byte and size
fn register_at(offset: u32) -> Option<(u32, u32)> {
    const SIZES: [(u32, u32); 36] = [
        (R_GCAP, 2), (R_VMIN, 1), (R_VMAJ, 1), (R_OUTPAY, 2), (R_INPAY, 2), (R_GCTL, 4), (R_WAKEEN, 2), (R_STATESTS, 2),
        (R_GSTS, 2), (R_OUTSTRMPAY, 2), (R_INSTRMPAY, 2), (R_INTCTL, 4), (R_INTSTS, 4), (R_WALCLK, 4), (R_SSYNC_OLD, 4),
        (R_SSYNC, 4), (R_CORBLBASE, 4), (R_CORBUBASE, 4), (R_CORBWP, 2), (R_CORBRP, 2), (R_CORBCTL, 1), (R_CORBSTS, 1),
        (R_CORBSIZE, 1), (R_RIRBLBASE, 4), (R_RIRBUBASE, 4), (R_RIRBWP, 2), (R_RINTCNT, 2), (R_RIRBCTL, 1), (R_RIRBSTS, 1),
        (R_RIRBSIZE, 1), (R_ICOI, 4), (R_ICII, 4), (R_ICIS, 2), (R_DPLBASE, 4), (R_DPUBASE, 4), (R_WALCLK_ALIAS, 4),
    ];
    const STREAM_SIZES: [(u32, u32); 10] = [
        (SD_CTL, 3), (SD_STS, 1), (SD_LPIB, 4), (SD_CBL, 4), (SD_LVI, 2), (SD_FIFOW, 2), (SD_FIFOS, 2), (SD_FMT, 2),
        (SD_BDPL, 4), (SD_BDPU, 4),
    ];
    let end = STREAMS as u32 * STREAM_SIZE;
    if offset >= R_STREAMS && offset < R_STREAMS + end {
        let local = offset - R_STREAMS & 0x1F;
        return STREAM_SIZES.iter().find(|&&(s, n)| local >= s && local < s + n).map(|&(s, n)| (offset - local + s, n));
    }
    if offset >= R_LPIB_ALIAS && offset < R_LPIB_ALIAS + end {
        let local = offset - R_LPIB_ALIAS & 0x1F;
        return if local < 4 { Some((offset - local, 4)) } else { None };
    }
    SIZES.iter().find(|&&(s, n)| offset >= s && offset < s + n).copied()
}

impl Hda {
    unsafe fn now() -> f64 { js::microtick() }

    unsafe fn is_bus_master(&self) -> bool { super::pci::command(self.pci_id) & 4 != 0 }

    unsafe fn reset_controller(&mut self) {
        for i in 0..STREAMS {
            self.stop(i);
        }
        for i in 0..STREAMS {
            self.streams[i] = Stream::new(i as u32);
        }
        self.wakeen = 0;
        self.statests = 0;
        self.gsts = 0;
        self.intctl = 0;
        self.ssync = 0;
        self.corb_base = [0; 2];
        self.corb_wp = 0;
        self.corb_rp = 0;
        self.corb_rp_reset = false;
        self.corbctl = 0;
        self.corbsts = 0;
        self.rirb_base = [0; 2];
        self.rirb_wp = 0;
        self.rintcnt = 0;
        self.rirbctl = 0;
        self.rirbsts = 0;
        self.rirb_count = 0;
        self.icoi = 0;
        self.icii = 0;
        self.icis = 0;
        self.dplbase = 0;
        self.dpubase = 0;
        self.wall_base = Self::now();
        self.codec = Codec::new();
        self.update_irq();
    }

    unsafe fn register(&self, offset: u32) -> u32 {
        let end = STREAMS as u32 * STREAM_SIZE;
        if offset >= R_STREAMS && offset < R_STREAMS + end {
            let s = &self.streams[((offset - R_STREAMS) >> 5) as usize];
            return match offset - R_STREAMS & 0x1F {
                SD_CTL => s.ctl,
                SD_STS => s.sts | if s.running { SD_STS_FIFORDY } else { 0 },
                SD_LPIB => s.lpib,
                SD_CBL => s.cbl,
                SD_LVI => s.lvi,
                SD_FIFOW => s.fifow,
                SD_FIFOS => FIFO_SIZE,
                SD_FMT => s.fmt,
                SD_BDPL => s.bdpl,
                SD_BDPU => s.bdpu,
                _ => 0,
            };
        }
        if offset >= R_LPIB_ALIAS && offset < R_LPIB_ALIAS + end && (offset - R_LPIB_ALIAS) & 0x1F == 0 {
            return self.streams[((offset - R_LPIB_ALIAS) >> 5) as usize].lpib;
        }
        match offset {
            R_GCAP => GCAP,
            R_VMIN => 0,
            R_VMAJ => 1,
            R_OUTPAY => 0x3C,
            R_INPAY => 0x1D,
            R_GCTL => self.gctl,
            R_WAKEEN => self.wakeen,
            R_STATESTS => self.statests,
            R_GSTS => self.gsts,
            R_OUTSTRMPAY => 0x30,
            R_INSTRMPAY => 0x18,
            R_INTCTL => self.intctl,
            R_INTSTS => self.interrupt_status(),
            R_WALCLK | R_WALCLK_ALIAS => self.wallclock(),
            R_SSYNC | R_SSYNC_OLD => self.ssync,
            R_CORBLBASE => self.corb_base[0],
            R_CORBUBASE => self.corb_base[1],
            R_CORBWP => self.corb_wp,
            R_CORBRP => self.corb_rp | if self.corb_rp_reset { CORBRP_RST } else { 0 },
            R_CORBCTL => self.corbctl,
            R_CORBSTS => self.corbsts,
            R_CORBSIZE => RING_SIZE,
            R_RIRBLBASE => self.rirb_base[0],
            R_RIRBUBASE => self.rirb_base[1],
            R_RIRBWP => self.rirb_wp,
            R_RINTCNT => self.rintcnt,
            R_RIRBCTL => self.rirbctl,
            R_RIRBSTS => self.rirbsts,
            R_RIRBSIZE => RING_SIZE,
            R_ICOI => self.icoi,
            R_ICII => self.icii,
            R_ICIS => self.icis,
            R_DPLBASE => self.dplbase,
            R_DPUBASE => self.dpubase,
            _ => 0,
        }
    }

    unsafe fn read(&mut self, offset: u32, size: u32) -> u32 {
        let mut value = 0u32;
        let mut i = 0;
        while i < size {
            let Some((start, length)) = register_at(offset + i)
            else {
                i += 1;
                continue;
            };
            let register = self.register(start);
            while i < size && offset + i < start + length {
                value |= (register >> ((offset + i - start) << 3) & 0xFF) << (i << 3);
                i += 1;
            }
        }
        value
    }

    unsafe fn write(&mut self, offset: u32, size: u32, value: u32) {
        let mut i = 0;
        while i < size {
            let Some((start, length)) = register_at(offset + i)
            else {
                i += 1;
                continue;
            };
            let (mut mask, mut bits) = (0u32, 0u32);
            while i < size && offset + i < start + length {
                let shift = (offset + i - start) << 3;
                mask |= 0xFF << shift;
                bits |= (value >> (i << 3) & 0xFF) << shift;
                i += 1;
            }
            self.write_register(start, bits, mask);
        }
    }

    unsafe fn write_register(&mut self, offset: u32, bits: u32, mask: u32) {
        let merge = |old: u32| old & !mask | bits & mask;
        let clear = |old: u32| old & !(bits & mask);
        if offset == R_GCTL {
            let v = merge(self.gctl);
            self.write_gctl(v);
            return;
        }
        if self.gctl & GCTL_CRST == 0 {
            return;
        }
        let end = STREAMS as u32 * STREAM_SIZE;
        if offset >= R_STREAMS && offset < R_STREAMS + end {
            self.write_stream(((offset - R_STREAMS) >> 5) as usize, offset - R_STREAMS & 0x1F, bits, mask);
            return;
        }
        match offset {
            R_WAKEEN => {
                self.wakeen = merge(self.wakeen) & 0x7FFF;
                self.update_irq();
            },
            R_STATESTS => {
                self.statests = clear(self.statests);
                self.update_irq();
            },
            R_GSTS => self.gsts = clear(self.gsts),
            R_INTCTL => {
                self.intctl = merge(self.intctl);
                self.update_irq();
            },
            R_SSYNC | R_SSYNC_OLD => {
                self.ssync = merge(self.ssync) & ((1 << STREAMS) - 1);
                for i in 0..STREAMS {
                    self.update_running(i);
                }
            },
            R_CORBLBASE => self.corb_base[0] = merge(self.corb_base[0]) & !0x7F,
            R_CORBUBASE => self.corb_base[1] = merge(self.corb_base[1]),
            R_CORBWP => {
                self.corb_wp = merge(self.corb_wp) & 0xFF;
                self.run_corb();
            },
            R_CORBRP => {
                if merge(0) & CORBRP_RST != 0 {
                    self.corb_rp = 0;
                    self.corb_rp_reset = true;
                }
                else {
                    self.corb_rp_reset = false;
                }
            },
            R_CORBCTL => {
                self.corbctl = merge(self.corbctl) & (CORBCTL_CMEIE | CORBCTL_RUN);
                self.run_corb();
            },
            R_CORBSTS => {
                self.corbsts = clear(self.corbsts);
                self.update_irq();
            },
            R_RIRBLBASE => self.rirb_base[0] = merge(self.rirb_base[0]) & !0x7F,
            R_RIRBUBASE => self.rirb_base[1] = merge(self.rirb_base[1]),
            R_RIRBWP => {
                if merge(0) & RIRBWP_RST != 0 {
                    self.rirb_wp = 0;
                }
            },
            R_RINTCNT => self.rintcnt = merge(self.rintcnt) & 0xFF,
            R_RIRBCTL => {
                self.rirbctl = merge(self.rirbctl) & 7;
                self.update_irq();
                self.run_corb();
            },
            R_RIRBSTS => {
                self.rirbsts = clear(self.rirbsts);
                self.update_irq();
            },
            R_ICOI => self.icoi = merge(self.icoi),
            R_ICIS => {
                let written = merge(0);
                if written & ICIS_IRV != 0 {
                    self.icis &= !ICIS_IRV;
                }
                if written & ICIS_ICB != 0 && self.icis & ICIS_ICB == 0 {
                    self.icis |= ICIS_ICB;
                    if let Some(response) = self.send_command(self.icoi) {
                        self.icii = response;
                        self.icis = self.icis & !ICIS_ICB | ICIS_IRV;
                    }
                }
            },
            R_DPLBASE => {
                self.dplbase = merge(self.dplbase) & !0x7E;
                self.write_positions();
            },
            R_DPUBASE => self.dpubase = merge(self.dpubase),
            _ => {},
        }
    }

    unsafe fn write_gctl(&mut self, value: u32) {
        let was = self.gctl & GCTL_CRST;
        if value & GCTL_CRST == 0 {
            self.reset_controller();
            self.gctl = 0;
            return;
        }
        self.gctl = value & (GCTL_CRST | GCTL_UNSOL);
        if was == 0 {
            self.statests |= 1 << Codec::ADDRESS;
            self.update_irq();
        }
    }

    fn send_command(&mut self, command: u32) -> Option<u32> {
        if command >> 28 != Codec::ADDRESS {
            return None;
        }
        Some(self.codec.command(command))
    }

    unsafe fn run_corb(&mut self) {
        while self.corbctl & CORBCTL_RUN != 0 && self.corb_rp != self.corb_wp {
            if !self.is_bus_master() {
                return;
            }
            let rp = self.corb_rp + 1 & 0xFF;
            let command = dma_read32(ring_address(self.corb_base) + 4 * rp as u64);
            self.corb_rp = rp;
            if let Some(response) = command.and_then(|c| self.send_command(c)) {
                self.respond(response);
            }
        }
    }

    unsafe fn respond(&mut self, response: u32) {
        if self.rirbctl & RIRBCTL_DMAEN == 0 {
            return;
        }
        let wp = self.rirb_wp + 1 & 0xFF;
        let address = ring_address(self.rirb_base) + 8 * wp as u64;
        let mut entry = [0u8; 8];
        entry[..4].copy_from_slice(&response.to_le_bytes());
        entry[4] = Codec::ADDRESS as u8;
        dma_write(&entry, address);
        self.rirb_wp = wp;
        self.rirb_count += 1;
        let threshold = if self.rintcnt != 0 { self.rintcnt } else { 256 };
        if self.rirb_count >= threshold || self.corb_rp == self.corb_wp {
            self.rirb_count = 0;
            if self.rirbctl & RIRBCTL_RINTCTL != 0 {
                self.rirbsts |= RIRBSTS_RINTFL;
                self.update_irq();
            }
        }
    }

    fn interrupt_status(&self) -> u32 {
        let mut status = 0;
        if self.rirbsts & (RIRBSTS_RINTFL | RIRBSTS_OIS) != 0
            || self.statests & self.wakeen != 0
            || self.corbsts & self.corbctl & CORBCTL_CMEIE != 0
        {
            status |= INT_CIE;
        }
        for s in &self.streams {
            let enabled = (if s.ctl & SD_CTL_IOCE != 0 { SD_STS_BCIS } else { 0 })
                | (if s.ctl & SD_CTL_FEIE != 0 { SD_STS_FIFOE } else { 0 })
                | (if s.ctl & SD_CTL_DEIE != 0 { SD_STS_DESE } else { 0 });
            if s.sts & enabled != 0 {
                status |= 1 << s.index;
            }
        }
        if status & self.intctl != 0 {
            status |= INT_GIE;
        }
        status
    }

    unsafe fn update_irq(&mut self) {
        let level = self.interrupt_status() & INT_GIE != 0 && self.intctl & INT_GIE != 0;
        if level == self.irq_level {
            return;
        }
        self.irq_level = level;
        if self.present {
            super::pci::set_irq(self.pci_id, level);
        }
    }

    unsafe fn write_stream(&mut self, i: usize, local: u32, bits: u32, mask: u32) {
        let merge = |old: u32| old & !mask | bits & mask;
        match local {
            SD_CTL => {
                let value = merge(self.streams[i].ctl) & 0xFF001F;
                if value & SD_CTL_SRST != 0 {
                    self.stop(i);
                    self.streams[i] = Stream { ctl: SD_CTL_SRST, ..Stream::new(i as u32) };
                    self.update_irq();
                    return;
                }
                self.streams[i].ctl = value;
                self.update_running(i);
                self.update_irq();
            },
            SD_STS => {
                self.streams[i].sts &= !(bits & mask & (SD_STS_BCIS | SD_STS_FIFOE | SD_STS_DESE));
                self.update_irq();
            },
            _ => {
                let s = &mut self.streams[i];
                match local {
                    SD_CBL if !s.running => s.cbl = merge(s.cbl),
                    SD_LVI if !s.running => s.lvi = merge(s.lvi) & 0xFF,
                    SD_FIFOW => s.fifow = merge(s.fifow) & 7,
                    SD_FMT if !s.running => s.fmt = merge(s.fmt) & 0x7F7F,
                    SD_BDPL if !s.running => s.bdpl = merge(s.bdpl) & !0x7F,
                    SD_BDPU if !s.running => s.bdpu = merge(s.bdpu),
                    _ => {},
                }
            },
        }
    }

    unsafe fn update_running(&mut self, i: usize) {
        let s = &self.streams[i];
        let run = s.ctl & SD_CTL_RUN != 0 && self.ssync & 1 << s.index == 0;
        if run == s.running {
            return;
        }
        if run {
            let now = Self::now();
            let s = &mut self.streams[i];
            s.running = true;
            s.started = now;
            s.played = 0.0;
            if s.output() {
                let (rate, _, _) = decode_format(s.fmt);
                if rate != self.speaker_rate {
                    self.speaker_rate = rate;
                    js::hda_event(EV_RATE, rate);
                }
                js::hda_event(EV_ENABLE, 0.0);
            }
            js::stop_idling();
        }
        else {
            self.play(i, Self::now());
            self.stop(i);
        }
    }

    unsafe fn stop(&mut self, i: usize) {
        if !self.streams[i].running {
            return;
        }
        self.streams[i].running = false;
        if self.streams[i].output() && !self.streams.iter().enumerate().any(|(j, o)| j != i && o.running && o.output()) {
            js::hda_event(EV_DISABLE, 0.0);
        }
    }

    unsafe fn timer(&mut self, now: f64) -> f64 {
        let mut next = 100.0f64;
        for i in 0..STREAMS {
            if !self.streams[i].running {
                continue;
            }
            self.play(i, now);
            next = next.min(self.time_to_boundary(i, now));
        }
        next
    }

    unsafe fn time_to_boundary(&self, i: usize, now: f64) -> f64 {
        let s = &self.streams[i];
        let (rate, bytes, channels) = decode_format(s.fmt);
        let left = match self.descriptor(s, s.entry) {
            Some((_, length, _)) => (length as f64 - s.offset as f64).max(0.0) / (bytes * channels) as f64,
            None => 0.0,
        };
        let due_at = s.started + (s.played + left) / rate * 1000.0;
        (due_at - now).min(10.0).max(0.5)
    }

    /// A buffer descriptor: address, length, IOC
    unsafe fn descriptor(&self, s: &Stream, index: u32) -> Option<(u64, u32, bool)> {
        let base = (s.bdpu as u64) << 32 | s.bdpl as u64;
        let b = dma_read(base + 16 * index as u64, 16)?;
        let word = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]);
        Some(((word(4) as u64) << 32 | word(0) as u64, word(8), word(12) & 1 != 0))
    }

    unsafe fn play(&mut self, i: usize, now: f64) {
        {
            let s = &self.streams[i];
            if !s.running || !s.output() || !self.is_bus_master() {
                return;
            }
        }
        let (rate, bytes, channels) = decode_format(self.streams[i].fmt);
        let frame_bytes = bytes * channels;
        let mut due = ((now - self.streams[i].started) / 1000.0 * rate).floor() - self.streams[i].played;
        if due <= 0.0 {
            return;
        }
        let max = (rate * MAX_PLAY_MS / 1000.0).floor();
        if due > rate * MAX_PLAY_MS / 1000.0 {
            let skip = due - max;
            self.streams[i].started += skip / rate * 1000.0;
            due -= skip;
        }
        let mut pcm: Vec<u8> = Vec::new();
        let mut interrupt = false;
        let mut guard = 0;
        while due > 0.0 && guard < 1024 {
            guard += 1;
            let s = self.streams[i];
            let entry = self.descriptor(&s, s.entry);
            let Some((address, length, ioc)) = entry.filter(|e| e.1 >= frame_bytes)
            else {
                let s = &mut self.streams[i];
                s.sts |= SD_STS_DESE;
                s.ctl &= !SD_CTL_RUN;
                self.stop(i);
                interrupt = true;
                break;
            };
            let take = (due * frame_bytes as f64).min((length - s.offset) as f64) as u32;
            let count = take - take % frame_bytes;
            let s = &mut self.streams[i];
            if count > 0 {
                match dma_read(address + s.offset as u64, count) {
                    Some(data) => pcm.extend_from_slice(data),
                    None => pcm.resize(pcm.len() + count as usize, 0),
                }
                s.offset += count;
                s.played += (count / frame_bytes) as f64;
                due -= (count / frame_bytes) as f64;
                s.lpib += count;
                if s.lpib >= s.cbl {
                    s.lpib = if s.cbl != 0 { s.lpib % s.cbl } else { 0 };
                }
            }
            if s.offset + frame_bytes > length {
                if ioc {
                    s.sts |= SD_STS_BCIS;
                    interrupt = true;
                }
                s.offset = 0;
                s.entry = if s.entry >= s.lvi { 0 } else { s.entry + 1 };
            }
        }
        self.write_positions();
        if !pcm.is_empty() {
            self.output(i, pcm, bytes, channels);
        }
        if interrupt {
            self.update_irq();
        }
    }

    unsafe fn write_positions(&mut self) {
        if self.dplbase & DPLBASE_ENABLE == 0 || !self.is_bus_master() {
            return;
        }
        let base = (self.dpubase as u64) << 32 | (self.dplbase & !0x7F) as u64;
        let mut bytes = [0u8; 8 * STREAMS];
        for s in &self.streams {
            bytes[8 * s.index as usize..8 * s.index as usize + 4].copy_from_slice(&s.lpib.to_le_bytes());
        }
        dma_write(&bytes, base);
    }

    unsafe fn output(&mut self, i: usize, pcm: Vec<u8>, bytes: u32, channels: u32) {
        let s = &self.streams[i];
        let tag = s.ctl >> 20 & 0xF;
        let gains = self.codec.output_for(tag).unwrap_or([0.0, 0.0]);
        let frame = (bytes * channels) as usize;
        let frames = pcm.len() / frame;
        let first = self.codec.channel.min(channels - 1) as usize;
        let second = (self.codec.channel + 1).min(channels - 1) as usize;
        let sample = |o: usize| -> f32 {
            match bytes {
                2 => i16::from_le_bytes([pcm[o], pcm[o + 1]]) as f32 / 32768.0,
                4 => (i32::from_le_bytes([pcm[o], pcm[o + 1], pcm[o + 2], pcm[o + 3]]) as f64 / 2147483648.0) as f32,
                _ => (pcm[o] as f32 - 128.0) / 128.0,
            }
        };
        self.out_left.clear();
        self.out_right.clear();
        for f in 0..frames {
            let o = f * frame;
            self.out_left.push(sample(o + first * bytes as usize) * gains[0]);
            self.out_right.push(sample(o + second * bytes as usize) * gains[1]);
        }
        self.out_info = [s.index, s.fmt, tag];
        self.out_pcm = pcm;
        js::hda_event(EV_OUTPUT, frames as f64);
    }

    unsafe fn wallclock(&self) -> u32 { ((Self::now() - self.wall_base) * WALLCLK_PER_MS).floor() as i64 as u32 }
}

// ---------------------------------------------------------------------------------------------------------------
// the core's interface

#[no_mangle]
pub unsafe fn hda_init(pci_id: i32) {
    let h = hda();
    h.present = true;
    h.pci_id = pci_id as usize & 0xFF;
    h.mmio = -1;
    MMIO_BASE = -1;
    h.reset_controller();
    h.gctl = 0;
}

/// A machine reset (PCIRST#)
#[no_mangle]
pub unsafe fn hda_reset() {
    let h = hda();
    h.reset_controller();
    h.gctl = 0;
}

/// Where BAR0 is decoded now (-1: not)
#[no_mangle]
pub unsafe fn hda_set_mmio(base: f64) {
    let h = hda();
    h.mmio = base as i64;
    MMIO_BASE = if h.present { base as i64 } else { -1 };
}

#[no_mangle]
pub unsafe fn hda_mmio_read(offset: u32, size: u32) -> u32 { hda().read(offset, size) }

#[no_mangle]
pub unsafe fn hda_mmio_write(offset: u32, size: u32, value: u32) { hda().write(offset, size, value) }

#[no_mangle]
pub unsafe fn hda_update_irq() { hda().update_irq() }

/// The last output's samples (left, then right) and the stream's bytes
#[no_mangle]
pub unsafe fn hda_output_left() -> u32 { hda().out_left.as_ptr() as u32 }
#[no_mangle]
pub unsafe fn hda_output_right() -> u32 { hda().out_right.as_ptr() as u32 }
#[no_mangle]
pub unsafe fn hda_output_pcm() -> u32 { hda().out_pcm.as_ptr() as u32 }
#[no_mangle]
pub unsafe fn hda_output_pcm_length() -> u32 { hda().out_pcm.len() as u32 }
/// 0: the stream's index, 1: its format, 2: its tag
#[no_mangle]
pub unsafe fn hda_output_info(i: u32) -> u32 { hda().out_info[i as usize % 3] }

#[inline]
pub unsafe fn serves(addr: u32) -> bool {
    MMIO_BASE >= 0 && (addr as i64) >= MMIO_BASE && (addr as i64) < MMIO_BASE + MMIO_SIZE as i64
}

pub unsafe fn read(addr: u32, size: u32) -> i32 {
    let h = hda();
    let offset = (addr as i64 - h.mmio) as u32;
    h.read(offset, size) as i32
}

pub unsafe fn write(addr: u32, size: u32, value: u32) {
    let h = hda();
    let offset = (addr as i64 - h.mmio) as u32;
    h.write(offset, size, value)
}

/// From the core's timer pass
pub unsafe fn timer(now: f64) -> f64 {
    #[allow(static_mut_refs)]
    match HDA.as_mut() {
        Some(h) if h.present => h.timer(now),
        _ => 100.0,
    }
}

// snapshots: the state as numbers, in the order of controller.js's get_state (times relative to now)
static mut STATE: Vec<f64> = Vec::new();
const STREAM_STATE: usize = 14;
const CODEC_STATE: usize = 17;

#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn hda_state_addr() -> u32 { STATE.as_ptr() as u32 }
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn hda_state_len() -> u32 { STATE.len() as u32 }
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn hda_state_resize(n: u32) -> u32 {
    STATE.clear();
    STATE.resize(n as usize, 0.0);
    STATE.as_ptr() as u32
}

/// registers (28, as get_state's Int32Array), the wall clock, each stream (14), the codec (17: power as 4)
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn hda_save() {
    let h = hda();
    let now = Hda::now();
    let s = &mut STATE;
    s.clear();
    for v in [
        h.gctl, h.wakeen, h.statests, h.gsts, h.intctl, h.ssync, h.corb_base[0], h.corb_base[1], h.corb_wp, h.corb_rp,
        h.corb_rp_reset as u32, h.corbctl, h.corbsts, h.rirb_base[0], h.rirb_base[1], h.rirb_wp, h.rintcnt, h.rirbctl,
        h.rirbsts, h.rirb_count, h.icoi, h.icii, h.icis, h.dplbase, h.dpubase,
    ] {
        s.push(v as i32 as f64);
    }
    s.push(h.mmio as f64); // (bar0: the plugin's)
    s.push(h.irq_level as u32 as f64);
    s.push(h.speaker_rate);
    s.push(now - h.wall_base);
    for st in &h.streams {
        for v in [st.ctl, st.sts, st.lpib, st.cbl, st.lvi, st.fifow, st.fmt, st.bdpl, st.bdpu, st.entry, st.offset] {
            s.push(v as i32 as f64);
        }
        s.push(now - st.started);
        s.push(st.played);
        s.push(st.running as u32 as f64);
    }
    let c = &h.codec;
    for v in [
        c.stream, c.channel, c.format, c.gain[0], c.gain[1], c.mute[0] as u32, c.mute[1] as u32, c.pin_control,
        c.pin_config, c.eapd, c.unsolicited, c.power[0], c.power[1], c.power[2], c.power[3], c.subsystem_id, c.digital,
    ] {
        s.push(v as f64);
    }
    let _ = (STREAM_STATE, CODEC_STATE);
}

/// hda_save's numbers back (in STATE); the speaker started again when a stream plays
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn hda_load() {
    let h = hda();
    let now = Hda::now();
    let s = &STATE;
    let u = |i: usize| s[i] as i64 as u32;
    h.gctl = u(0);
    h.wakeen = u(1);
    h.statests = u(2);
    h.gsts = u(3);
    h.intctl = u(4);
    h.ssync = u(5);
    h.corb_base = [u(6), u(7)];
    h.corb_wp = u(8);
    h.corb_rp = u(9);
    h.corb_rp_reset = u(10) != 0;
    h.corbctl = u(11);
    h.corbsts = u(12);
    h.rirb_base = [u(13), u(14)];
    h.rirb_wp = u(15);
    h.rintcnt = u(16);
    h.rirbctl = u(17);
    h.rirbsts = u(18);
    h.rirb_count = u(19);
    h.icoi = u(20);
    h.icii = u(21);
    h.icis = u(22);
    h.dplbase = u(23);
    h.dpubase = u(24);
    h.irq_level = u(26) != 0;
    h.speaker_rate = s[27];
    h.wall_base = now - s[28];
    let mut at = 29;
    for i in 0..STREAMS {
        let v = |k: usize| s[at + k] as i64 as u32;
        h.streams[i] = Stream {
            index: i as u32,
            ctl: v(0),
            sts: v(1),
            lpib: v(2),
            cbl: v(3),
            lvi: v(4),
            fifow: v(5),
            fmt: v(6),
            bdpl: v(7),
            bdpu: v(8),
            entry: v(9),
            offset: v(10),
            started: now - s[at + 11],
            played: s[at + 12],
            running: s[at + 13] != 0.0,
        };
        at += 14;
    }
    let v = |k: usize| s[at + k] as i64 as u32;
    h.codec = Codec {
        stream: v(0),
        channel: v(1),
        format: v(2),
        gain: [v(3), v(4)],
        mute: [v(5) != 0, v(6) != 0],
        pin_control: v(7),
        pin_config: v(8),
        eapd: v(9),
        unsolicited: v(10),
        power: [v(11), v(12), v(13), v(14)],
        subsystem_id: v(15),
        digital: v(16),
    };
    if h.streams.iter().any(|s| s.running && s.output()) {
        js::hda_event(EV_RATE, h.speaker_rate);
        js::hda_event(EV_ENABLE, 0.0);
    }
    if h.present {
        super::pci::set_irq(h.pci_id, h.irq_level);
    }
}
