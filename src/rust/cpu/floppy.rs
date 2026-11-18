// Floppy disk controller (82078): its registers, the command, execution and result phases and every command, and
// the drives' geometry and head positions. Was src/floppy.js, which keeps the disk images (inserting, ejecting,
// finding the format by size), the data transfers of READ and WRITE through DMA (asynchronous, the images are
// JavaScript's buffers: a command leaves a transfer here that JavaScript runs after the slice and reports back with
// fdc_transfer_done) and the snapshot format.
// https://github.com/qemu/qemu/blob/master/hw/block/fdc.c

use crate::cpu::cpu::{device_lower_irq, device_raise_irq, js};

const FDC_IRQ_CHANNEL: u8 = 6;

const SRA_INTPEND: i32 = 0x80;
const SRB_MTR0: i32 = 0x1;
const SRB_MTR1: i32 = 0x2;
const SRB_DR0: i32 = 0x20;
const SRB_RESET: i32 = 0xc0;
const DOR_SEL_LO: i32 = 0x1;
const DOR_SEL_HI: i32 = 0x2;
const DOR_NRESET: i32 = 0x4;
const DOR_DMAEN: i32 = 0x8;
const DOR_MOTEN0: i32 = 0x10;
const DOR_MOTEN1: i32 = 0x20;
const DOR_SELMASK: i32 = 0x01;
const TDR_BOOTSEL: i32 = 0x4;
const MSR_CMDBUSY: i32 = 0x10;
const MSR_NDMA: i32 = 0x20;
const MSR_DIO: i32 = 0x40;
const MSR_RQM: i32 = 0x80;
const DSR_DRATEMASK: i32 = 0x3;
const DSR_PWRDOWN: i32 = 0x40;
const DSR_SWRESET: i32 = 0x80;
const DIR_DOOR: i32 = 0x80;
const SR0_DS0: i32 = 0x1;
const SR0_DS1: i32 = 0x2;
const SR0_HEAD: i32 = 0x4;
const SR0_SEEK: i32 = 0x20;
const SR0_ABNTERM: i32 = 0x40;
const SR0_INVCMD: i32 = 0x80;
const SR0_RDYCHG: i32 = SR0_ABNTERM | SR0_INVCMD;
const SR1_MA: i32 = 0x1;
const SR1_NW: i32 = 0x2;
const SR1_EC: i32 = 0x80;

const CMD_READ_TRACK: i32 = 0x2;
const CMD_SPECIFY: i32 = 0x3;
const CMD_SENSE_DRIVE_STATUS: i32 = 0x4;
const CMD_WRITE: i32 = 0x5;
const CMD_READ: i32 = 0x6;
const CMD_RECALIBRATE: i32 = 0x7;
const CMD_SENSE_INTERRUPT_STATUS: i32 = 0x8;
const CMD_WRITE_DELETED_DATA: i32 = 0x9;
const CMD_READ_ID: i32 = 0xa;
const CMD_READ_DELETED_DATA: i32 = 0xc;
const CMD_FORMAT_TRACK: i32 = 0xd;
const CMD_DUMP_REGS: i32 = 0xe;
const CMD_SEEK: i32 = 0xf;
const CMD_VERSION: i32 = 0x10;
const CMD_SCAN_EQUAL: i32 = 0x11;
const CMD_PERPENDICULAR_MODE: i32 = 0x12;
const CMD_CONFIGURE: i32 = 0x13;
const CMD_LOCK: i32 = 0x14;
const CMD_VERIFY: i32 = 0x16;
const CMD_POWERDOWN_MODE: i32 = 0x17;
const CMD_PART_ID: i32 = 0x18;
const CMD_SCAN_LOW_OR_EQUAL: i32 = 0x19;
const CMD_SCAN_HIGH_OR_EQUAL: i32 = 0x1d;
const CMD_SAVE: i32 = 0x2e;
const CMD_OPTION: i32 = 0x33;
const CMD_RESTORE: i32 = 0x4e;
const CMD_DRIVE_SPECIFICATION: i32 = 0x8e;
const CMD_RELATIVE_SEEK_OUT: i32 = 0x8f;
const CMD_FORMAT_AND_WRITE: i32 = 0xcd;
const CMD_RELATIVE_SEEK_IN: i32 = 0xcf;
const CMD_UNKNOWN: i32 = -1;

const CMD_FLAG_MULTI_TRACK: i32 = 0x1;
const CMD_PHASE_COMMAND: i32 = 1;
const CMD_PHASE_EXECUTION: i32 = 2;
const CMD_PHASE_RESULT: i32 = 3;
const CONFIG_EFIFO: i32 = 0x20;
const CONFIG_EIS: i32 = 0x40;
const RESET_SENSE_INT_MAX: i32 = 4;
const SECTOR_SIZE_CODE: u8 = 2;

/// (code, mask, argument count) in the order of matching: the first whose (byte & mask) == code
/// (NOTE: the order is significant, as in qemu's table)
const COMMANDS: [(i32, i32, i32); 31] = [
    (CMD_READ, 0x1f, 8),
    (CMD_WRITE, 0x3f, 8),
    (CMD_SEEK, 0xff, 2),
    (CMD_SENSE_INTERRUPT_STATUS, 0xff, 0),
    (CMD_RECALIBRATE, 0xff, 1),
    (CMD_FORMAT_TRACK, 0xbf, 5),
    (CMD_READ_TRACK, 0xbf, 8),
    (CMD_RESTORE, 0xff, 17),
    (CMD_SAVE, 0xff, 0),
    (CMD_READ_DELETED_DATA, 0x1f, 8),
    (CMD_SCAN_EQUAL, 0x1f, 8),
    (CMD_VERIFY, 0x1f, 8),
    (CMD_SCAN_LOW_OR_EQUAL, 0x1f, 8),
    (CMD_SCAN_HIGH_OR_EQUAL, 0x1f, 8),
    (CMD_WRITE_DELETED_DATA, 0x3f, 8),
    (CMD_READ_ID, 0xbf, 1),
    (CMD_SPECIFY, 0xff, 2),
    (CMD_SENSE_DRIVE_STATUS, 0xff, 1),
    (CMD_PERPENDICULAR_MODE, 0xff, 1),
    (CMD_CONFIGURE, 0xff, 3),
    (CMD_POWERDOWN_MODE, 0xff, 2),
    (CMD_OPTION, 0xff, 1),
    (CMD_DRIVE_SPECIFICATION, 0xff, 5),
    (CMD_RELATIVE_SEEK_OUT, 0xff, 2),
    (CMD_FORMAT_AND_WRITE, 0xff, 10),
    (CMD_RELATIVE_SEEK_IN, 0xff, 2),
    (CMD_LOCK, 0x7f, 0),
    (CMD_DUMP_REGS, 0xff, 0),
    (CMD_VERSION, 0xff, 0),
    (CMD_PART_ID, 0xff, 0),
    (CMD_UNKNOWN, 0x00, 0),
];

/// the command a byte starts and its argument count
fn command(byte: i32) -> (i32, i32) {
    for &(code, mask, argc) in COMMANDS.iter() {
        if mask == 0 || byte & mask == code {
            return (code, argc);
        }
    }
    (CMD_UNKNOWN, 0)
}

// (the layout is read by src/floppy.js: eleven 32-bit fields per drive)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Drive {
    drive_type: i32,    // 0
    max_track: i32,     // 1
    max_head: i32,      // 2
    max_sect: i32,      // 3
    curr_track: i32,    // 4
    curr_head: i32,     // 5
    curr_sect: i32,     // 6
    perpendicular: i32, // 7
    read_only: i32,     // 8
    media_changed: i32, // 9
    /// a disk image is inserted (JavaScript has it)
    has_media: i32, // 10
}

impl Drive {
    fn chs2lba(&self, track: i32, head: i32, sect: i32) -> i32 { (track * self.max_head + head) * self.max_sect + sect - 1 }

    /// 0: the same track, 1: the track changed, 2: head or track out of range or no disk, 3: sector out of range
    fn seek(&mut self, head: i32, track: i32, sect: i32) -> i32 {
        if track > self.max_track || head != 0 && self.max_head == 1 {
            return 2;
        }
        if sect > self.max_sect {
            return 3;
        }
        let mut result = 0;
        let curr_lba = self.chs2lba(self.curr_track, self.curr_head, self.curr_sect);
        let new_lba = self.chs2lba(track, head, sect);
        if curr_lba != new_lba {
            if self.curr_track != track {
                if self.has_media != 0 {
                    self.media_changed = 0;
                }
                result = 1;
            }
            self.curr_head = head;
            self.curr_track = track;
            self.curr_sect = sect;
        }
        if self.has_media == 0 {
            result = 2;
        }
        result
    }
}

// (the layout is read by src/floppy.js)
#[repr(C)]
pub struct Fdc {
    sra: i32,                   // 0
    srb: i32,                   // 1
    dor: i32,                   // 2
    tdr: i32,                   // 3
    msr: i32,                   // 4
    dsr: i32,                   // 5
    cmd_phase: i32,             // 6
    cmd_code: i32,              // 7
    cmd_flags: i32,             // 8
    cmd_cursor: i32,            // 9
    cmd_remaining: i32,         // 10
    response_cursor: i32,       // 11
    response_length: i32,       // 12
    status0: i32,               // 13
    status1: i32,               // 14
    curr_drive_no: i32,         // 15
    reset_sense_int_count: i32, // 16
    locked: i32,                // 17
    step_rate_interval: i32,    // 18
    head_load_time: i32,        // 19
    fdc_config: i32,            // 20
    precomp_trk: i32,           // 21
    eot: i32,                   // 22
    present: i32,               // 23
    /// a READ or WRITE waiting for JavaScript's DMA: 1 read, 2 write; the byte offset and length in the image
    xfer: i32, // 24
    xfer_offset: i32,           // 25
    xfer_length: i32,           // 26
    _pad: i32,                  // 27
    cmd_buffer: [u8; 17],       // 112
    response_data: [u8; 15],    // 129
    drives: [Drive; 2],         // 144
}

const DRIVE0: Drive = Drive {
    drive_type: 0,
    max_track: 0,
    max_head: 0,
    max_sect: 0,
    curr_track: 0,
    curr_head: 0,
    curr_sect: 1,
    perpendicular: 0,
    read_only: 0,
    media_changed: 1,
    has_media: 0,
};

const FDC0: Fdc = Fdc {
    sra: 0,
    srb: SRB_RESET,
    dor: DOR_NRESET | DOR_DMAEN,
    tdr: 0,
    msr: MSR_RQM,
    dsr: 0,
    cmd_phase: CMD_PHASE_COMMAND,
    cmd_code: 0,
    cmd_flags: 0,
    cmd_cursor: 0,
    cmd_remaining: 0,
    response_cursor: 0,
    response_length: 0,
    status0: 0,
    status1: 0,
    curr_drive_no: 0,
    reset_sense_int_count: 0,
    locked: 0,
    step_rate_interval: 0,
    head_load_time: 0,
    fdc_config: CONFIG_EIS | CONFIG_EFIFO,
    precomp_trk: 0,
    eot: 0,
    present: 0,
    xfer: 0,
    xfer_offset: 0,
    xfer_length: 0,
    _pad: 0,
    cmd_buffer: [0; 17],
    response_data: [0; 15],
    drives: [DRIVE0; 2],
};

static mut FDC: Fdc = FDC0;

#[allow(static_mut_refs)]
fn fdc() -> &'static mut Fdc { unsafe { &mut FDC } }

#[no_mangle]
pub fn get_fdc_addr() -> u32 { &raw mut FDC as u32 }

#[no_mangle]
pub unsafe fn fdc_init() {
    *fdc() = FDC0;
    fdc().present = 1;
}

impl Fdc {
    fn drive(&mut self) -> &mut Drive { &mut self.drives[self.curr_drive_no as usize & 1] }

    fn select(&mut self, no: i32) -> &mut Drive {
        self.curr_drive_no = no & 1;
        self.drive()
    }

    unsafe fn raise_irq(&mut self) {
        if self.sra & SRA_INTPEND == 0 {
            device_raise_irq(FDC_IRQ_CHANNEL);
            self.sra |= SRA_INTPEND;
        }
        self.reset_sense_int_count = 0;
    }

    unsafe fn lower_irq(&mut self) {
        self.status0 = 0;
        if self.sra & SRA_INTPEND != 0 {
            device_lower_irq(FDC_IRQ_CHANNEL);
            self.sra &= !SRA_INTPEND;
        }
    }

    fn enter_command_phase(&mut self) {
        self.cmd_phase = CMD_PHASE_COMMAND;
        self.cmd_cursor = 0;
        self.cmd_remaining = 0;
        self.msr &= !(MSR_CMDBUSY | MSR_DIO);
        self.msr |= MSR_RQM;
    }

    fn enter_result_phase(&mut self, fifo_len: i32) {
        self.cmd_phase = CMD_PHASE_RESULT;
        self.response_cursor = 0;
        self.response_length = fifo_len;
        self.msr |= MSR_CMDBUSY | MSR_RQM | MSR_DIO;
    }

    unsafe fn reset_fdc(&mut self) {
        self.lower_irq();
        // NOTE: set SRA to SRA_NDRV2 if fdb does not exist
        self.sra = 0;
        self.srb = SRB_RESET;
        self.dor = DOR_NRESET | DOR_DMAEN;
        self.msr = MSR_RQM;
        self.curr_drive_no = 0;
        self.status0 |= SR0_RDYCHG;
        self.response_cursor = 0;
        self.response_length = 0;
        self.drives[0].seek(0, 0, 1);
        self.drives[1].seek(0, 0, 1);
        self.enter_command_phase();
        self.raise_irq();
        self.reset_sense_int_count = RESET_SENSE_INT_MAX;
    }

    unsafe fn read(&mut self, reg: i32) -> i32 {
        match reg {
            0 => self.sra,
            1 => self.srb,
            2 => self.dor & !(DOR_SEL_LO | DOR_SEL_HI) | self.curr_drive_no,
            3 => self.tdr,
            4 => {
                self.dsr &= !DSR_PWRDOWN;
                self.dor |= DOR_NRESET;
                self.msr
            },
            5 => {
                self.dsr &= !DSR_PWRDOWN;
                if self.msr & MSR_RQM == 0 || self.msr & MSR_DIO == 0 || self.cmd_phase != CMD_PHASE_RESULT {
                    return 0;
                }
                if self.response_cursor < self.response_length {
                    let byte = self.response_data[self.response_cursor as usize] as i32;
                    self.response_cursor += 1;
                    if self.response_cursor == self.response_length {
                        self.msr &= !MSR_RQM;
                        self.enter_command_phase();
                        self.lower_irq();
                    }
                    byte
                }
                else {
                    0
                }
            },
            7 => {
                if self.drive().media_changed != 0 { DIR_DOOR } else { 0 }
            },
            _ => 0xFF,
        }
    }

    unsafe fn write(&mut self, reg: i32, byte: i32) {
        match reg {
            2 => {
                self.srb = self.srb & !(SRB_MTR0 | SRB_MTR1 | SRB_DR0)
                    | if byte & DOR_MOTEN0 != 0 { SRB_MTR0 } else { 0 }
                    | if byte & DOR_MOTEN1 != 0 { SRB_MTR1 } else { 0 }
                    | if byte & DOR_SEL_LO != 0 { SRB_DR0 } else { 0 };
                if self.dor & DOR_NRESET == 0 && byte & DOR_NRESET != 0 {
                    self.reset_fdc();
                    self.dsr &= !DSR_PWRDOWN;
                }
                self.curr_drive_no = byte & DOR_SEL_LO;
                self.dor = byte;
            },
            3 => {
                if self.dor & DOR_NRESET != 0 {
                    // Disk boot selection indicator
                    self.tdr = byte & TDR_BOOTSEL;
                }
            },
            4 => {
                if self.dor & DOR_NRESET == 0 {
                    return;
                }
                if byte & DSR_SWRESET != 0 {
                    self.dor &= !DOR_NRESET;
                    self.reset_fdc();
                    self.dor |= DOR_NRESET;
                }
                if byte & DSR_PWRDOWN != 0 {
                    self.reset_fdc();
                }
                self.dsr = byte;
            },
            5 => self.write_fifo(byte),
            7 => {
                if self.dor & DOR_NRESET != 0 {
                    self.dsr = self.dsr & !DSR_DRATEMASK | byte & DSR_DRATEMASK;
                }
            },
            _ => {},
        }
    }

    unsafe fn write_fifo(&mut self, byte: i32) {
        self.dsr &= !DSR_PWRDOWN;
        if self.dor & DOR_NRESET == 0 || self.msr & MSR_RQM == 0 || self.msr & MSR_DIO != 0 || self.cmd_phase != CMD_PHASE_COMMAND {
            return;
        }
        if self.cmd_remaining == 0 {
            let (code, argc) = command(byte);
            self.cmd_code = byte;
            self.cmd_remaining = argc;
            self.cmd_cursor = 0;
            self.cmd_flags = 0;
            // 0x80: Multi-track (MT)
            if (code == CMD_READ || code == CMD_WRITE) && self.cmd_code & 0x80 != 0 {
                self.cmd_flags |= CMD_FLAG_MULTI_TRACK;
            }
            if self.cmd_remaining != 0 {
                self.msr |= MSR_RQM;
            }
            self.msr |= MSR_CMDBUSY;
        }
        else {
            self.cmd_buffer[self.cmd_cursor as usize] = byte as u8;
            self.cmd_cursor += 1;
            self.cmd_remaining -= 1;
        }
        if self.cmd_remaining == 0 {
            self.cmd_phase = CMD_PHASE_EXECUTION;
            let args: [i32; 17] = std::array::from_fn(|i| self.cmd_buffer[i] as i32);
            self.execute(command(self.cmd_code).0, &args);
        }
    }

    unsafe fn execute(&mut self, code: i32, args: &[i32; 17]) {
        match code {
            CMD_READ => self.start_read_write(args, false),
            CMD_WRITE => self.start_read_write(args, true),
            CMD_SEEK => {
                let track = args[1];
                let d = self.select(args[0] & DOR_SELMASK);
                let (head, sect) = (d.curr_head, d.curr_sect);
                d.seek(head, track, sect);
                self.enter_command_phase();
                self.status0 |= SR0_SEEK;
                self.raise_irq();
            },
            CMD_SENSE_INTERRUPT_STATUS => {
                let status0;
                if self.reset_sense_int_count > 0 {
                    let drv_nr = RESET_SENSE_INT_MAX - self.reset_sense_int_count;
                    self.reset_sense_int_count -= 1;
                    status0 = SR0_RDYCHG | drv_nr;
                }
                else if self.sra & SRA_INTPEND != 0 {
                    status0 = self.status0 & !(SR0_HEAD | SR0_DS1 | SR0_DS0) | self.curr_drive_no;
                }
                else {
                    self.response_data[0] = SR0_INVCMD as u8;
                    self.enter_result_phase(1);
                    return;
                }
                self.response_data[0] = status0 as u8;
                self.response_data[1] = self.drive().curr_track as u8;
                self.enter_result_phase(2);
                self.lower_irq();
                self.status0 = SR0_RDYCHG;
            },
            CMD_RECALIBRATE => {
                self.select(args[0] & DOR_SELMASK).seek(0, 0, 1);
                self.enter_command_phase();
                self.status0 |= SR0_SEEK;
                self.raise_irq();
            },
            CMD_FORMAT_TRACK => {
                let d = self.select(args[0] & DOR_SELMASK);
                let (status0, status1) = if d.read_only != 0 { (SR0_ABNTERM | SR0_SEEK, SR1_NW) } else { (0, 0) };
                self.end_read_write(status0, status1, 0);
            },
            CMD_READ_ID => {
                let d = self.drive();
                d.curr_head = args[0] >> 2 & 1;
                if d.max_sect != 0 {
                    d.curr_sect = d.curr_sect % d.max_sect + 1;
                }
                self.end_read_write(0, 0, 0);
            },
            CMD_SPECIFY => {
                // 0..3: Head Unload Time (HUT), 4..7: Step Rate Interval (SRT); 0: Non-DMA mode flag (ND), 1..7: HLT
                self.step_rate_interval = args[0] >> 4;
                self.head_load_time = args[1] >> 1;
                if args[1] & 1 != 0 {
                    self.dor &= !DOR_DMAEN;
                }
                else {
                    self.dor |= DOR_DMAEN;
                }
                self.enter_command_phase();
            },
            CMD_SENSE_DRIVE_STATUS => {
                let drv_sel = args[0];
                let no = drv_sel & DOR_SELMASK & 1;
                let d = self.select(drv_sel & DOR_SELMASK);
                d.curr_head = drv_sel >> 2 & 1;
                // response byte is Status Register 3; bits 3 and 5 unused, always "1"
                let sr3 = (if d.read_only != 0 { 0x40 } else { 0 })
                    | (if d.curr_track == 0 { 0x10 } else { 0 })
                    | d.curr_head << 2
                    | no
                    | 0x28;
                self.response_data[0] = sr3 as u8;
                self.enter_result_phase(1);
            },
            CMD_PERPENDICULAR_MODE => {
                // 0x80: OW, bits D0 and D1 can be overwritten
                if args[0] & 0x80 != 0 {
                    self.drive().perpendicular = args[0] & 0x7;
                }
                self.enter_command_phase();
            },
            CMD_CONFIGURE => {
                self.fdc_config = args[1];
                self.precomp_trk = args[2];
                self.enter_command_phase();
            },
            CMD_LOCK => {
                if self.cmd_code & 0x80 != 0 {
                    self.locked = 1;
                    self.response_data[0] = 0x10;
                }
                else {
                    self.locked = 0;
                    self.response_data[0] = 0;
                }
                self.enter_result_phase(1);
            },
            CMD_DUMP_REGS => {
                let (t0, t1) = (self.drives[0].curr_track, self.drives[1].curr_track);
                let (max_sect, perpendicular) = (self.drive().max_sect, self.drive().perpendicular);
                let r = [
                    t0,
                    t1,
                    0,
                    0,
                    self.step_rate_interval,
                    self.head_load_time << 1 | (self.dor & DOR_DMAEN != 0) as i32,
                    max_sect,
                    (if self.locked != 0 { 0x80 } else { 0 }) | perpendicular << 2,
                    self.fdc_config,
                    self.precomp_trk,
                ];
                for (i, v) in r.iter().enumerate() {
                    self.response_data[i] = *v as u8;
                }
                self.enter_result_phase(10);
            },
            // 0x80: Standard controller, 0x81: Intel 82077, 0x90: Intel 82078
            CMD_VERSION => {
                self.response_data[0] = 0x90;
                self.enter_result_phase(1);
            },
            // Stepping 1 (PS/2 mode)
            CMD_PART_ID => {
                self.response_data[0] = 0x41;
                self.enter_result_phase(1);
            },
            _ => {
                // unimplemented
                self.status0 = SR0_INVCMD;
                self.response_data[0] = self.status0 as u8;
                self.enter_result_phase(1);
            },
        }
    }

    unsafe fn fail_at(&mut self, status0: i32, status1: i32, track: i32, head: i32, sect: i32) {
        self.end_read_write(status0, status1, 0);
        self.response_data[3] = track as u8;
        self.response_data[4] = head as u8;
        self.response_data[5] = sect as u8;
    }

    unsafe fn start_read_write(&mut self, args: &[i32; 17], do_write: bool) {
        let (track, head, sect) = (args[1], args[2], args[3]);
        // sector size code 0..7 (0:128, 1:256, 2:512, ..., 7:16384 bytes/sect)
        let ssc = args[4];
        // last sector number of current track
        let eot = args[5];
        // data length in bytes if ssc is 0, else unused
        let dtl = if args[7] < 128 { args[7] } else { 128 };

        match self.select(args[0] & DOR_SELMASK).seek(head, track, sect) {
            // error: sect too big
            2 => return self.fail_at(SR0_ABNTERM, 0, track, head, sect),
            // error: track too big
            3 => return self.fail_at(SR0_ABNTERM, SR1_EC, track, head, sect),
            // track changed
            1 => self.status0 |= SR0_SEEK,
            _ => {},
        }

        let sect_size = 128 << ssc.min(7);
        let sect_start = self.drive().chs2lba(track, head, sect);
        let data_offset = sect_start * sect_size;
        let data_length;
        if sect_size == 128 {
            data_length = dtl;
        }
        else {
            data_length = if self.cmd_flags & CMD_FLAG_MULTI_TRACK != 0 {
                (2 * eot - sect + 1) * sect_size
            }
            else {
                (eot - sect + 1) * sect_size
            };
            if data_length <= 0 {
                return self.fail_at(SR0_ABNTERM, SR1_MA, track, head, sect);
            }
        }
        self.eot = eot;

        if do_write && self.drive().read_only != 0 {
            return self.fail_at(SR0_ABNTERM | SR0_SEEK, SR1_NW, track, head, sect);
        }

        if self.dor & DOR_DMAEN != 0 {
            self.msr &= !MSR_RQM;
            self.xfer = if do_write { 2 } else { 1 };
            self.xfer_offset = data_offset;
            self.xfer_length = data_length;
        }
        // (PIO mode read/write is not supported: the command stays in its execution phase, as in JavaScript)
    }

    unsafe fn end_read_write(&mut self, status0: i32, status1: i32, status2: i32) {
        let no = self.curr_drive_no;
        let (track, head, sect) = (self.drive().curr_track, self.drive().curr_head, self.drive().curr_sect);
        self.status0 &= !(SR0_DS0 | SR0_DS1 | SR0_HEAD);
        self.status0 |= no;
        if head != 0 {
            self.status0 |= SR0_HEAD;
        }
        self.status0 |= status0;
        self.msr |= MSR_RQM | MSR_DIO;
        self.msr &= !MSR_NDMA;
        let r = [self.status0, status1, status2, track, head, sect, SECTOR_SIZE_CODE as i32];
        for (i, v) in r.iter().enumerate() {
            self.response_data[i] = *v as u8;
        }
        self.enter_result_phase(7);
        self.raise_irq();
    }

    fn seek_next_sect(&mut self) {
        let eot = self.eot;
        let multi_track = self.cmd_flags & CMD_FLAG_MULTI_TRACK != 0;
        let d = self.drives[self.curr_drive_no as usize & 1];
        let (mut new_track, mut new_head, mut new_sect) = (d.curr_track, d.curr_head, d.curr_sect);
        if new_sect >= d.max_sect || new_sect == eot {
            new_sect = 1;
            if multi_track {
                if new_head == 0 && d.max_head == 2 {
                    new_head = 1;
                }
                else {
                    new_head = 0;
                    new_track += 1;
                    self.status0 |= SR0_SEEK;
                }
            }
            else {
                self.status0 |= SR0_SEEK;
                new_track += 1;
            }
        }
        else {
            new_sect += 1;
        }
        self.drive().seek(new_head, new_track, new_sect);
    }
}

/// JavaScript ran the DMA of a READ or WRITE
#[no_mangle]
pub unsafe fn fdc_transfer_done(error: bool) {
    let f = fdc();
    f.xfer = 0;
    if error {
        f.end_read_write(SR0_ABNTERM, 0, 0);
    }
    else {
        f.seek_next_sect();
        f.end_read_write(0, 0, 0);
    }
}

/// Ports 0x3F0-0x3F7, also for src/floppy.js
#[no_mangle]
pub unsafe fn fdc_port_read(port: i32) -> i32 { fdc().read(port & 7) }

#[no_mangle]
pub unsafe fn fdc_port_write(port: i32, value: i32) {
    fdc().write(port & 7, value & 0xFF);
    if fdc().xfer != 0 {
        // the transfer of a READ or WRITE starts at once (as the original device did): waiting for the timer pass
        // left a guest that halts after the command asleep until the next timer interrupt
        fdc_run_transfer();
    }
}

/// Whether the core answers this port itself (0x3F6 is the IDE's)
#[inline]
pub unsafe fn serves(port: i32, write: bool) -> bool {
    if port & !7 != 0x3F0 || fdc().present == 0 || super::parallel::WORKER {
        return false;
    }
    let reg = port & 7;
    if write { matches!(reg, 2 | 3 | 4 | 5 | 7) } else { matches!(reg, 0..=5 | 7) }
}

// ---------------------------------------------------------------------------------------------------------------
// The data of a READ or WRITE through DMA channel 2, after the slice that ran the command (fdc_run_transfer, in the
// core's timer pass): the image's bytes come from JavaScript (js::floppy_io kind 0, answered with fdc_io_buffer and
// fdc_read_done), written ones go to it (kind 1, then fdc_write_done).

const FDC_DMA_CHANNEL: usize = 2;
static mut IMAGE_BYTES: [f64; 2] = [0.0; 2];
static mut RUNNING: bool = false;
static mut READ_TO: u32 = 0;
static mut IO: Vec<u8> = Vec::new();

/// The size of a drive's image (0: none)
#[no_mangle]
pub unsafe fn fdc_image_size(drive: i32, bytes: f64) { IMAGE_BYTES[drive as usize & 1] = bytes; }

/// From the core's timer pass (after each slice): a transfer the command left starts
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn fdc_run_transfer() -> f64 {
    let f = fdc();
    if f.xfer == 0 || RUNNING {
        return 100.0;
    }
    let image = IMAGE_BYTES[f.curr_drive_no as usize & 1];
    let offset = f.xfer_offset as usize;
    if f.xfer == 1 {
        let (addr, count) = super::dma::transfer_from_device(FDC_DMA_CHANNEL);
        if (offset + count) as f64 > image {
            fdc_transfer_done(true);
            return 100.0;
        }
        super::dma::step_address(FDC_DMA_CHANNEL, count);
        RUNNING = true;
        READ_TO = addr;
        js::floppy_io(0, f.curr_drive_no, offset as f64, 0, count as i32);
    }
    else {
        let room = (image - offset as f64).max(0.0) as usize;
        let mut out = vec![0u8; room];
        match super::dma::transfer_to_device_n(FDC_DMA_CHANNEL, &mut out, 0, f.xfer_length.max(0) as usize) {
            None => fdc_transfer_done(true),
            Some(n) => {
                out.truncate(n);
                IO = out;
                RUNNING = true;
                js::floppy_io(1, f.curr_drive_no, offset as f64, IO.as_ptr() as u32, n as i32);
            },
        }
    }
    100.0
}

/// Room for the bytes of a read
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn fdc_io_buffer(length: i32) -> u32 {
    IO.clear();
    IO.resize(length.max(0) as usize, 0);
    IO.as_mut_ptr() as u32
}

/// The bytes of the read are in fdc_io_buffer: into memory
#[no_mangle]
#[allow(static_mut_refs)]
pub unsafe fn fdc_read_done() {
    if !RUNNING {
        return;
    }
    RUNNING = false;
    let size = *crate::cpu::global_pointers::memory_size as usize;
    let start = (READ_TO as usize).min(size);
    let end = (READ_TO as usize + IO.len()).min(size);
    if end > start {
        std::ptr::copy_nonoverlapping(IO.as_ptr(), crate::cpu::memory::mem8.add(start), end - start);
        crate::jit::jit_dirty_cache(start as u32, end as u32);
    }
    fdc_transfer_done(false);
}

/// The written bytes are in the image
#[no_mangle]
pub unsafe fn fdc_write_done() {
    if !RUNNING {
        return;
    }
    RUNNING = false;
    fdc_transfer_done(false);
}
