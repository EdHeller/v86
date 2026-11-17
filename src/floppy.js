// v86 Floppy Disk Controller emulation
//
// This file is licensed under both BSD and MIT, see LICENSE and LICENSE.MIT.
//
// Links
// - Intel 82078 44 Pin CHMOS Single-Chip Floppy Disk Controller
//   https://wiki.qemu.org/images/f/f0/29047403.pdf
// - qemu: fdc.c
//   https://github.com/qemu/qemu/blob/master/hw/block/fdc.c
// - Programming Floppy Disk Controllers
//   https://www.isdaman.com/alsos/hardware/fdc/floppy.htm
// - OSDev: Floppy Disk Controller
//   https://wiki.osdev.org/Floppy_Disk_Controller

import { LOG_FLOPPY } from "./const.js";
import { h } from "./lib.js";
import { dbg_assert, dbg_log } from "./log.js";
import { CMOS_FLOPPY_DRIVE_TYPE } from "./rtc.js";
import { SyncBuffer } from "./buffer.js";

// For Types Only
import { CPU } from "./cpu.js";
import { DMA } from "./dma.js";
import { IO } from "./io.js";

// System resources
const FDC_IRQ_CHANNEL = 6;
const FDC_DMA_CHANNEL = 2;

/**
 * Floppy drive types
 * CMOS register 0x10 bits: upper nibble: fda, lower nibble: fdb
 * @see {@link https://wiki.osdev.org/CMOS#Register_0x10}
 */
const CMOS_FDD_TYPE_NO_DRIVE = 0x0; // no floppy drive
const CMOS_FDD_TYPE_360      = 0x1; // 360 KB 5"1/4 drive
const CMOS_FDD_TYPE_1200     = 0x2; // 1.2 MB 5"1/4 drive
const CMOS_FDD_TYPE_720      = 0x3; // 720 KB 3"1/2 drive
const CMOS_FDD_TYPE_1440     = 0x4; // 1.44 MB 3"1/2 drive
const CMOS_FDD_TYPE_2880     = 0x5; // 2.88 MB 3"1/2 drive

// Physical floppy disk size identifier of each drive type
const CMOS_FDD_TYPE_MEDIUM = {
    [CMOS_FDD_TYPE_NO_DRIVE]: 0,    // no disk
    [CMOS_FDD_TYPE_360]:      525,  // 5"1/4 disk
    [CMOS_FDD_TYPE_1200]:     525,  // 5"1/4 disk
    [CMOS_FDD_TYPE_720]:      350,  // 3"1/2 disk
    [CMOS_FDD_TYPE_1440]:     350,  // 3"1/2 disk
    [CMOS_FDD_TYPE_2880]:     350,  // 3"1/2 disk
};

// Floppy Controller PIO Register offsets (base: 0x3F0/0x370, offset 0x6 is reserved for ATA IDE)
const REG_SRA       = 0x0;  // R,  Status Register A (SRA)
const REG_SRB       = 0x1;  // R,  Status Register B (SRB)
const REG_DOR       = 0x2;  // RW, Digital Output Register (DOR)
const REG_TDR       = 0x3;  // RW, Tape Drive Register (TDR)
const REG_MSR       = 0x4;  // R,  Main Status Register (MSR)
const REG_DSR       = 0x4;  // W,  Datarate Select Register (DSR)
const REG_FIFO      = 0x5;  // RW, W: command bytes, R: response bytes (FIFO)
const REG_DIR       = 0x7;  // R,  Digital Input Register (DIR)
const REG_CCR       = 0x7;  // W,  Configuration Control Register (CCR)

// Status Register A (SRA) bits
const SRA_NDRV2     = 0x40; // true: second drive is not connected
const SRA_INTPEND   = 0x80; // true: interrupt pending

// Status Register B (SRB) bits
const SRB_MTR0      = 0x1;  // follows DOR.DOR_MOT0
const SRB_MTR1      = 0x2;  // follows DOR.DOR_MOT1
const SRB_DR0       = 0x20; // follows DOR.DOR_SEL_LO
const SRB_RESET     = 0xc0; // magic value after reset

// Digital Output Register (DOR) bits
const DOR_SEL_LO    = 0x1;  // lower bit of selected FDD number
const DOR_SEL_HI    = 0x2;  // upper bit of selected FDD number
const DOR_NRESET    = 0x4;  // true: normal controller mode, false: reset mode ("not RESET")
const DOR_DMAEN     = 0x8;  // true: use DMA
const DOR_MOTEN0    = 0x10; // true: enable motor of FDD0
const DOR_MOTEN1    = 0x20; // true: enable motor of FDD1
const DOR_MOTEN2    = 0x40; // true: enable motor of FDD2
const DOR_MOTEN3    = 0x80; // true: enable motor of FDD3
const DOR_SELMASK   = 0x01;

// Tape Drive Register (TDR) bits
const TDR_BOOTSEL   = 0x4;

// Main Status Register (MSR) bits
const MSR_FDD0      = 0x1;  // true: FDD0 is busy in seek mode
const MSR_FDD1      = 0x2;  // true: FDD1 is busy in seek mode
const MSR_FDD2      = 0x4;  // true: FDD2 is busy in seek mode
const MSR_FDD3      = 0x8;  // true: FDD3 is busy in seek mode
const MSR_CMDBUSY   = 0x10; // true: FDC busy, Read/Write command in progress, cleared at end of Result phase
const MSR_NDMA      = 0x20; // Non-DMA mode, set in Execution phase of PIO mode read/write commands only.
const MSR_DIO       = 0x40; // Data Input/Output, true: FDC has data for CPU, false: FDC expects data from CPU
const MSR_RQM       = 0x80; // true: DATA register is ready for I/O

// Datarate Select Register (DSR) bits
const DSR_DRATEMASK = 0x3;
const DSR_PWRDOWN   = 0x40;
const DSR_SWRESET   = 0x80;

// Digital Input Register (DIR) bits
const DIR_DOOR      = 0x80; // true: No disk or disk changed since last command

// Status Register 0 (SR0) bits
const SR0_DS0       = 0x1;  // Drive select 0..3 lower bit
const SR0_DS1       = 0x2;  // Drive select 0..3 upper bit
const SR0_HEAD      = 0x4;  // true: Use 2nd head
const SR0_EQPMT     = 0x10; // (?)
const SR0_SEEK      = 0x20; // (?)
const SR0_ABNTERM   = 0x40; // true: Command failed
const SR0_INVCMD    = 0x80; // true: Unknown/unimplemented command code
const SR0_RDYCHG    = SR0_ABNTERM | SR0_INVCMD; // 0xC0 (?)

// Status Register 1 (SR1) bits
const SR1_MA        = 0x1;  // true: Missing address mark error
const SR1_NW        = 0x2;  // true: Not writable error
const SR1_EC        = 0x80; // true: End of cylinder error

// Status Register 2 (SR2) bits
const SR2_SNS       = 0x4;  // true: Scan not satisfied (?)
const SR2_SEH       = 0x8;  // true: Scan equal hit (?)

/**
 * FDC command codes
 * We declare all known floppy commands but implement only the subset that
 * we actually observe in the field. See also build_cmd_lookup_table().
 * @see {@link https://github.com/qemu/qemu/blob/6e1571533fd92bec67e5ab9b1dd1e15032925757/hw/block/fdc.c#L619}
 */
const CMD_READ_TRACK             = 0x2; // unimplemented
const CMD_SPECIFY                = 0x3;
const CMD_SENSE_DRIVE_STATUS     = 0x4;
const CMD_WRITE                  = 0x5;
const CMD_READ                   = 0x6;
const CMD_RECALIBRATE            = 0x7;
const CMD_SENSE_INTERRUPT_STATUS = 0x8;
const CMD_WRITE_DELETED_DATA     = 0x9; // unimplemented
const CMD_READ_ID                = 0xa;
const CMD_READ_DELETED_DATA      = 0xc; // unimplemented
const CMD_FORMAT_TRACK           = 0xd;
const CMD_DUMP_REGS              = 0xe;
const CMD_SEEK                   = 0xf;
const CMD_VERSION                = 0x10;
const CMD_SCAN_EQUAL             = 0x11; // unimplemented
const CMD_PERPENDICULAR_MODE     = 0x12;
const CMD_CONFIGURE              = 0x13;
const CMD_LOCK                   = 0x14;
const CMD_VERIFY                 = 0x16; // unimplemented
const CMD_POWERDOWN_MODE         = 0x17; // unimplemented
const CMD_PART_ID                = 0x18;
const CMD_SCAN_LOW_OR_EQUAL      = 0x19; // unimplemented
const CMD_SCAN_HIGH_OR_EQUAL     = 0x1d; // unimplemented
const CMD_SAVE                   = 0x2e; // unimplemented
const CMD_OPTION                 = 0x33; // unimplemented
const CMD_RESTORE                = 0x4e; // unimplemented
const CMD_DRIVE_SPECIFICATION    = 0x8e; // unimplemented
const CMD_RELATIVE_SEEK_OUT      = 0x8f; // unimplemented
const CMD_FORMAT_AND_WRITE       = 0xcd; // unimplemented
const CMD_RELATIVE_SEEK_IN       = 0xcf; // unimplemented

// FDC command flags
const CMD_FLAG_MULTI_TRACK  = 0x1;  // MT: multi-track selector (use both heads) in READ/WRITE

// FDC command execution phases
const CMD_PHASE_COMMAND     = 1;
const CMD_PHASE_EXECUTION   = 2;
const CMD_PHASE_RESULT      = 3;

// FDC config bits
const CONFIG_PRETRK     = 0xff; // Pre-compensation set to track 0
const CONFIG_FIFOTHR    = 0x0f; // FIFO threshold set to 1 byte
const CONFIG_POLL       = 0x10; // Poll enabled
const CONFIG_EFIFO      = 0x20; // FIFO disabled
const CONFIG_EIS        = 0x40; // No implied seeks

// Number of CMD_SENSE_INTERRUPT_STATUS expected after reset
const RESET_SENSE_INT_MAX = 4;

// Sector size
const SECTOR_SIZE       = 512;  // fixed size of 512 bytes/sector
const SECTOR_SIZE_CODE  = 2;    // sector size code 2: 512 bytes/sector

// class FloppyController ----------------------------------------------------

// struct Fdc in src/rust/cpu/floppy.rs: 28 32-bit fields, cmd_buffer at 112, response_data at 129, the drives at 144
const FDC_FIELDS = [
    "sra", "srb", "dor", "tdr", "msr", "dsr", "cmd_phase", "cmd_code", "cmd_flags", "cmd_cursor", "cmd_remaining",
    "response_cursor", "response_length", "status0", "status1", "curr_drive_no", "reset_sense_int_count", "locked",
    "step_rate_interval", "head_load_time", "fdc_config", "precomp_trk", "eot",
];
const FDC_XFER = 24, FDC_XFER_OFFSET = 25, FDC_XFER_LENGTH = 26;
const FDC_DRIVES = 144, FDC_DRIVE_SIZE = 44;
// struct Drive: eleven 32-bit fields
const DRIVE_FIELDS = [
    "drive_type", "max_track", "max_head", "max_sect", "curr_track", "curr_head", "curr_sect", "perpendicular",
    "read_only", "media_changed",
];
const DRIVE_HAS_MEDIA = 10;

/**
 * Turns fields of an object into accessors of 32-bit fields of the core's memory (made at each use: it can grow)
 * @param {!Object} object
 * @param {?} cpu
 * @param {number} address
 * @param {!Array<string>} names in the order of the fields
 * @param {!Object<string, boolean>=} booleans fields that read as booleans
 */
function core_fields(object, cpu, address, names, booleans)
{
    names.forEach((name, index) => {
        const as_boolean = booleans && booleans[name];
        Object.defineProperty(object, name, {
            get: () => {
                const v = new Int32Array(cpu.wasm_memory.buffer, address, index + 1)[index];
                return as_boolean ? !!v : v;
            },
            set: value => { new Int32Array(cpu.wasm_memory.buffer, address, index + 1)[index] = +value; },
        });
    });
}

/**
 * The floppy disk controller. Its registers and commands are in the core (src/rust/cpu/floppy.rs), which answers
 * ports 0x3F0-0x3F7. Here: the drives' disk images, the DMA transfers of READ and WRITE (the core leaves them,
 * this runs them after the slice and reports back) and the snapshot format.
 * @constructor
 * @param {CPU} cpu
 * @param {Uint8Array|SyncBuffer|undefined} fda_image
 * @param {Uint8Array|SyncBuffer|undefined} fdb_image
 * @param {Object=} fdc_config
 */
export function FloppyController(cpu, fda_image, fdb_image, fdc_config)
{
    /** @const @type {IO|undefined} */
    this.io = cpu.io;

    /** @const @type {CPU} */
    this.cpu = cpu;

    /** @const @type {DMA} */
    this.dma = cpu.devices.dma;

    cpu.fdc_init();
    const base = cpu.get_fdc_addr();
    this.base = base;
    core_fields(this, cpu, base, FDC_FIELDS, { "locked": true });
    for(const [name, offset, length] of [["cmd_buffer", 112, 17], ["response_data", 129, 15]])
    {
        Object.defineProperty(this, name, { get: () => new Uint8Array(cpu.wasm_memory.buffer, base + offset, length) });
    }

    this.drives = [
        new FloppyDrive(cpu, base + FDC_DRIVES, "fda", fdc_config?.fda, fda_image, CMOS_FDD_TYPE_1440),
        new FloppyDrive(cpu, base + FDC_DRIVES + FDC_DRIVE_SIZE, "fdb", fdc_config?.fdb, fdb_image, CMOS_FDD_TYPE_1440),
    ];
    this.transfer_running = false;

    this.cpu.devices.rtc.cmos_write(CMOS_FLOPPY_DRIVE_TYPE, (this.drives[0].drive_type << 4) | this.drives[1].drive_type);

    const fdc_io_base = 0x3F0;  // alt: 0x370
    for(const reg of [REG_SRA, REG_SRB, REG_DOR, REG_TDR, REG_MSR, REG_FIFO, REG_DIR])
    {
        this.io.register_read(fdc_io_base | reg, this, () => cpu.fdc_port_read(fdc_io_base | reg));
    }
    for(const reg of [REG_DOR, REG_TDR, REG_DSR, REG_FIFO, REG_CCR])
    {
        this.io.register_write(fdc_io_base | reg, this, value => { cpu.fdc_port_write(fdc_io_base | reg, value); });
    }
}

/**
 * The core's READ or WRITE wants the image (src/rust/cpu/floppy.rs, floppy_io): kind 0 its bytes at offset, which
 * go back to the core; kind 1 bytes to write there
 */
FloppyController.prototype.core_io = function(kind, drive_no, offset, data, length)
{
    const drive = this.drives[drive_no];
    const exports = this.cpu.wm.exports;
    if(kind === 0)
    {
        drive.buffer.get(offset, length, bytes =>
        {
            const at = exports["fdc_io_buffer"](bytes.length) >>> 0;
            new Uint8Array(this.cpu.wasm_memory.buffer, at, bytes.length).set(bytes);
            exports["fdc_read_done"]();
        });
    }
    else
    {
        drive.buffer.set(offset, new Uint8Array(this.cpu.wasm_memory.buffer, data, length).slice(), () => exports["fdc_write_done"]());
    }
};

FloppyController.prototype.get_state = function()
{
    const state = [];
    FDC_FIELDS.forEach((name, i) => { state[19 + i] = this[name]; });
    state[28] = this.cmd_buffer.slice();
    state[29] = this.cmd_cursor;
    state[30] = this.cmd_remaining;
    state[31] = this.response_data.slice();
    state[32] = this.response_cursor;
    state[33] = this.response_length;
    state[34] = this.status0;
    state[35] = this.status1;
    state[36] = this.curr_drive_no;
    state[37] = this.reset_sense_int_count;
    state[38] = this.locked;
    state[39] = this.step_rate_interval;
    state[40] = this.head_load_time;
    state[41] = this.fdc_config;
    state[42] = this.precomp_trk;
    state[43] = this.eot;
    state[44] = this.drives[0].get_state();
    state[45] = this.drives[1].get_state();
    return state;
};

FloppyController.prototype.set_state = function(state)
{
    if(typeof state[19] === "undefined")
    {
        // old state format
        return;
    }
    this.sra = state[19];
    this.srb = state[20];
    this.dor = state[21];
    this.tdr = state[22];
    this.msr = state[23];
    this.dsr = state[24];
    this.cmd_phase = state[25];
    this.cmd_code = state[26];
    this.cmd_flags = state[27];
    this.cmd_buffer.set(state[28]);
    this.cmd_cursor = state[29];
    this.cmd_remaining = state[30];
    this.response_data.set(state[31]);
    this.response_cursor = state[32];
    this.response_length = state[33];
    this.status0 = state[34];
    this.status1 = state[35];
    this.curr_drive_no = state[36];
    this.reset_sense_int_count = state[37];
    this.locked = state[38];
    this.step_rate_interval = state[39];
    this.head_load_time = state[40];
    this.fdc_config = state[41];
    this.precomp_trk = state[42];
    this.eot = state[43];
    this.drives[0].set_state(state[44]);
    this.drives[1].set_state(state[45]);
};

/**
 * Floppy disk formats
 *
 * In many cases, the total sector count (sectors*tracks*heads) of a format
 * is enough to uniquely identify it. However, there are some total sector
 * collisions between formats of different physical size (drive_type), these
 * are highlighed below in the "collides" column.
 *
 * Matches that are higher up in the table take precedence over later matches.
 *
 * @see {@link https://github.com/qemu/qemu/blob/e240f6cc25917f3138d9e95e0343ae23b63a3f8c/hw/block/fdc.c#L99}
 * @see {@link https://en.wikipedia.org/wiki/List_of_floppy_disk_formats}
 */
const DISK_FORMATS = [
    //                                                                 ttl_sect     size     collides
    // 1.44 MB 3"1/2 floppy disks                                             |     |        |
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 18, tracks: 80, heads: 2 }, // 2880  1.44 MB  3.5"
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 20, tracks: 80, heads: 2 }, // 3200  1.6 MB   3.5"
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 21, tracks: 80, heads: 2 }, // 3360  1.68 MB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 21, tracks: 82, heads: 2 }, // 3444  1.72 MB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 21, tracks: 83, heads: 2 }, // 3486  1.74 MB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 22, tracks: 80, heads: 2 }, // 3520  1.76 MB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 23, tracks: 80, heads: 2 }, // 3680  1.84 MB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 24, tracks: 80, heads: 2 }, // 3840  1.92 MB
    // 2.88 MB 3"1/2 floppy disks
    { drive_type: CMOS_FDD_TYPE_2880, sectors: 36, tracks: 80, heads: 2 }, // 5760  2.88 MB
    { drive_type: CMOS_FDD_TYPE_2880, sectors: 39, tracks: 80, heads: 2 }, // 6240  3.12 MB
    { drive_type: CMOS_FDD_TYPE_2880, sectors: 40, tracks: 80, heads: 2 }, // 6400  3.2 MB
    { drive_type: CMOS_FDD_TYPE_2880, sectors: 44, tracks: 80, heads: 2 }, // 7040  3.52 MB
    { drive_type: CMOS_FDD_TYPE_2880, sectors: 48, tracks: 80, heads: 2 }, // 7680  3.84 MB
    // 720 kB 3"1/2 floppy disks
    { drive_type: CMOS_FDD_TYPE_1440, sectors:  8, tracks: 80, heads: 2 }, // 1280  640 kB
    { drive_type: CMOS_FDD_TYPE_1440, sectors:  9, tracks: 80, heads: 2 }, // 1440  720 kB   3.5"
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 10, tracks: 80, heads: 2 }, // 1600  800 kB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 10, tracks: 82, heads: 2 }, // 1640  820 kB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 10, tracks: 83, heads: 2 }, // 1660  830 kB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 13, tracks: 80, heads: 2 }, // 2080  1.04 MB
    { drive_type: CMOS_FDD_TYPE_1440, sectors: 14, tracks: 80, heads: 2 }, // 2240  1.12 MB
    // 1.2 MB 5"1/4 floppy disks
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 15, tracks: 80, heads: 2 }, // 2400  1.2 MB
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 18, tracks: 80, heads: 2 }, // 2880  1.44 MB  5.25"
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 18, tracks: 82, heads: 2 }, // 2952  1.48 MB
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 18, tracks: 83, heads: 2 }, // 2988  1.49 MB
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 20, tracks: 80, heads: 2 }, // 3200  1.6 MB   5.25"
    // 720 kB 5"1/4 floppy disks
    { drive_type: CMOS_FDD_TYPE_1200, sectors:  9, tracks: 80, heads: 2 }, // 1440  720 kB   5.25"
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 11, tracks: 80, heads: 2 }, // 1760  880 kB
    // 360 kB 5"1/4 floppy disks
    { drive_type: CMOS_FDD_TYPE_1200, sectors:  9, tracks: 40, heads: 2 }, // 720   360 kB   5.25"
    { drive_type: CMOS_FDD_TYPE_1200, sectors:  9, tracks: 40, heads: 1 }, // 360   180 kB
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 10, tracks: 41, heads: 2 }, // 820   410 kB
    { drive_type: CMOS_FDD_TYPE_1200, sectors: 10, tracks: 42, heads: 2 }, // 840   420 kB
    // 320 kB 5"1/4 floppy disks
    { drive_type: CMOS_FDD_TYPE_1200, sectors:  8, tracks: 40, heads: 2 }, // 640   320 kB
    { drive_type: CMOS_FDD_TYPE_1200, sectors:  8, tracks: 40, heads: 1 }, // 320   160 kB
    // 360 kB must match 5"1/4 better than 3"1/2...
    { drive_type: CMOS_FDD_TYPE_1440, sectors:  9, tracks: 80, heads: 1 }, // 720   360 kB   3.5"
    // types defined in earlier v86 releases (not defined in qemu)
    { drive_type: CMOS_FDD_TYPE_360,  sectors: 10, tracks: 40, heads: 1 }, // 400   200 kB
    { drive_type: CMOS_FDD_TYPE_360,  sectors: 10, tracks: 40, heads: 2 }, // 800   400 kB
];

/**
 * @constructor
 *
 * @param {string} name
 * @param {Object|undefined} fdd_config
 * @param {SyncBuffer|Uint8Array|null|undefined} buffer
 * @param {number} fallback_drive_type
 */
function FloppyDrive(cpu, address, name, fdd_config, buffer, fallback_drive_type)
{
    // (the geometry and head position are in the core: struct Drive in src/rust/cpu/floppy.rs)
    core_fields(this, cpu, address, DRIVE_FIELDS, { "read_only": true, "media_changed": true });
    let image = null;
    Object.defineProperty(this, "buffer", {
        get: () => image,
        set: value => {
            image = value;
            new Int32Array(cpu.wasm_memory.buffer, address, DRIVE_HAS_MEDIA + 1)[DRIVE_HAS_MEDIA] = value ? 1 : 0;
            // (the core moves the data of READ and WRITE: it checks them against the image's size)
            cpu.wm.exports["fdc_image_size"](name === "fdb" ? 1 : 0, value ? value.byteLength : 0);
        },
    });

    /** @const */
    this.name = name;

    // drive state
    this.drive_type = CMOS_FDD_TYPE_NO_DRIVE;

    // disk state
    this.max_track = 0;     // disk's max. track, was: FloppyController.number_of_cylinders (qemu: max_track)
    this.max_head = 0;      // disk's max. head (1 or 2), was: FloppyController.number_of_heads
    this.max_sect = 0;      // disk's max. sect, was: FloppyController.sectors_per_track (qemu: last_sect)
    this.curr_track = 0;    // >= 0, was: FloppyController.last_cylinder (qemu: track)
    this.curr_head = 0;     // 0 or 1, was: FloppyController.last_head (qemu: head)
    this.curr_sect = 1;     // > 0, was: FloppyController.last_sector (qemu: sect)
    this.perpendicular = 0;
    this.read_only = false;
    this.media_changed = true;
    this.buffer = null;


    // Drive type this.drive_type is either (in this order):
    // - specified in fdd_config.drive_type (if defined),
    // - derived from given image buffer (if provided), or
    // - specfied in fallback_drive_type.
    // If buffer is undefined and fdd_config.drive_type is
    // CMOS_FDD_TYPE_NO_DRIVE then the drive will be invisible to the guest.
    const cfg_drive_type = fdd_config?.drive_type;
    if(cfg_drive_type !== undefined && cfg_drive_type >= 0 && cfg_drive_type <= 5)
    {
        this.drive_type = cfg_drive_type;
    }

    this.insert_disk(buffer, !! fdd_config?.read_only);

    if(this.drive_type === CMOS_FDD_TYPE_NO_DRIVE && cfg_drive_type === undefined)
    {
        this.drive_type = fallback_drive_type;
    }

    dbg_log("floppy drive " + this.name + " ready, drive type: " + this.drive_type, LOG_FLOPPY);
}

/**
 * Insert disk image into floppy drive.
 *
 * @param {SyncBuffer|Uint8Array|null|undefined} buffer
 * @param {boolean=} read_only
 * @return {boolean} true if the given buffer was accepted
 */
FloppyDrive.prototype.insert_disk = function(buffer, read_only)
{
    if(!buffer)
    {
        return false;
    }

    if(buffer instanceof Uint8Array)
    {
        buffer = new SyncBuffer(buffer.buffer);
    }

    const [new_buffer, disk_format] = this.find_disk_format(buffer, this.drive_type);
    if(!new_buffer)
    {
        dbg_log("WARNING: disk rejected, no suitable disk format found for image of size " +
            buffer.byteLength + " bytes", LOG_FLOPPY);
        return false;
    }

    this.max_track = disk_format.tracks;
    this.max_head = disk_format.heads;
    this.max_sect = disk_format.sectors;
    this.read_only = !!read_only;
    this.media_changed = true;
    this.buffer = new_buffer;

    if(this.drive_type === CMOS_FDD_TYPE_NO_DRIVE)
    {
        // auto-select drive type once during construction
        this.drive_type = disk_format.drive_type;
    }

    if(DEBUG)
    {
        dbg_log("disk inserted into " + this.name + ": type: " + disk_format.drive_type +
            ", C/H/S: " + disk_format.tracks + "/" + disk_format.heads + "/" +
            disk_format.sectors + ", size: " + new_buffer.byteLength,
            LOG_FLOPPY);
    }
    return true;
};

/**
 * Eject disk from this floppy drive.
 */
FloppyDrive.prototype.eject_disk = function()
{
    this.max_track = 0;
    this.max_head = 0;
    this.max_sect = 0;
    this.media_changed = true;
    this.buffer = null;
};

/**
 * Returns this drive's current image buffer or null if no disk is inserted.
 * @return {Uint8Array|null}
 */
FloppyDrive.prototype.get_buffer = function()
{
    return this.buffer ? new Uint8Array(this.buffer.buffer) : null;
};

/**
 * Map structured C/H/S address to linear block address (LBA).
 * @param {number} track
 * @param {number} head
 * @param {number} sect
 * @return {number} lba the linear block address
 */
FloppyDrive.prototype.chs2lba = function(track, head, sect)
{
    return (track * this.max_head + head) * this.max_sect + sect - 1;
};

/**
 * Find best-matching disk format for the given image buffer.
 *
 * If the given drive_type is CMOS_FDD_TYPE_NO_DRIVE then drive types are
 * ignored and the first matching disk format is selected (auto-detect).
 *
 * If the size of the given buffer does not match any known floppy disk
 * format then the smallest format larger than the image buffer is
 * selected, and buffer is copied into a new, format-sized buffer with
 * trailing zeroes.
 *
 * Returns [null, null] if the given buffer is larger than any known
 * floppy disk format.
 *
 * @param {SyncBuffer} buffer
 * @param {number} drive_type
 * @return [{SyncBuffer|null}, {Object|null}] [buffer, disk_format]
 */
FloppyDrive.prototype.find_disk_format = function(buffer, drive_type)
{
    const autodetect = drive_type === CMOS_FDD_TYPE_NO_DRIVE;
    const buffer_size = buffer.byteLength;

    let preferred_match=-1, medium_match=-1, size_match=-1, nearest_match=-1, nearest_size=-1;
    for(let i = 0; i < DISK_FORMATS.length; i++)
    {
        const disk_format = DISK_FORMATS[i];
        const disk_size = disk_format.sectors * disk_format.tracks * disk_format.heads * SECTOR_SIZE;
        if(buffer_size === disk_size)
        {
            if(autodetect || disk_format.drive_type === drive_type)
            {
                // (1) same size and CMOS drive type
                preferred_match = i;
                break;
            }
            else if(!autodetect && CMOS_FDD_TYPE_MEDIUM[disk_format.drive_type] === CMOS_FDD_TYPE_MEDIUM[drive_type])
            {
                // (2) same size and physical medium size (5"1/4 or 3"1/2)
                medium_match = (medium_match === -1) ? i : medium_match;
            }
            else
            {
                // (3) same size
                size_match = (size_match === -1) ? i : size_match;
            }
        }
        else if(buffer_size < disk_size)
        {
            if(nearest_size === -1 || disk_size < nearest_size)
            {
                // (4) nearest matching size
                nearest_match = i;
                nearest_size = disk_size;
            }
        }
    }

    if(preferred_match !== -1)
    {
        return [buffer, DISK_FORMATS[preferred_match]];
    }
    else if(medium_match !== -1)
    {
        return [buffer, DISK_FORMATS[medium_match]];
    }
    else if(size_match !== -1)
    {
        return [buffer, DISK_FORMATS[size_match]];
    }
    else if(nearest_match !== -1)
    {
        const tmp_buffer = new Uint8Array(nearest_size);
        tmp_buffer.set(new Uint8Array(buffer.buffer));
        return [new SyncBuffer(tmp_buffer.buffer), DISK_FORMATS[nearest_match]];
    }
    else
    {
        return [null, null];
    }
};

/**
 * Seek to a new position, returns:
 *   0 if already on right track
 *   1 if track changed
 *   2 if track is invalid
 *   3 if sector is invalid
 *
 * @param {number} head
 * @param {number} track
 * @param {number} sect
 * @return {number}
 */
FloppyDrive.prototype.seek = function(head, track, sect)
{
    if((track > this.max_track) || (head !== 0 && this.max_head === 1))
    {
        dbg_log("WARNING: attempt to seek to invalid track: head: " + head + ", track: " + track + ", sect: " + sect, LOG_FLOPPY);
        return 2;
    }
    if(sect > this.max_sect)
    {
        dbg_log("WARNING: attempt to seek beyond last sector: " + sect + " (max: " + this.max_sect + ")", LOG_FLOPPY);
        return 3;
    }

    let result = 0;
    const curr_lba = this.chs2lba(this.curr_track, this.curr_head, this.curr_sect);
    const new_lba = this.chs2lba(track, head, sect);
    if(curr_lba !== new_lba)
    {
        if(this.curr_track !== track)
        {
            if(this.buffer)
            {
                this.media_changed = false;
            }
            result = 1;
        }
        this.curr_head = head;
        this.curr_track = track;
        this.curr_sect = sect;
    }

    if(!this.buffer)
    {
        result = 2;
    }

    return result;
};

FloppyDrive.prototype.get_state = function()
{
    const state = [];
    state[0]  = this.drive_type;
    state[1]  = this.max_track;
    state[2]  = this.max_head;
    state[3]  = this.max_sect;
    state[4]  = this.curr_track;
    state[5]  = this.curr_head;
    state[6]  = this.curr_sect;
    state[7]  = this.perpendicular;
    state[8]  = this.read_only;
    state[9]  = this.media_changed;
    state[10] = this.buffer ? this.buffer.get_state() : null;   // SyncBuffer
    return state;
};

FloppyDrive.prototype.set_state = function(state)
{
    this.drive_type = state[0];
    this.max_track = state[1];
    this.max_head = state[2];
    this.max_sect = state[3];
    this.curr_track = state[4];
    this.curr_head = state[5];
    this.curr_sect = state[6];
    this.perpendicular = state[7];
    this.read_only = state[8];
    this.media_changed = state[9];
    if(state[10])
    {
        if(!this.buffer)
        {
            this.buffer = new SyncBuffer(new ArrayBuffer(0));
        }
        this.buffer.set_state(state[10]);
    }
    else
    {
        this.buffer = null;
    }
};
