import { v86 } from "./main.js";
import { dbg_assert } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";
import { DMA } from "./dma.js";


export const CMOS_RTC_SECONDS = 0x00;
export const CMOS_RTC_SECONDS_ALARM = 0x01;
export const CMOS_RTC_MINUTES = 0x02;
export const CMOS_RTC_MINUTES_ALARM = 0x03;
export const CMOS_RTC_HOURS = 0x04;
export const CMOS_RTC_HOURS_ALARM = 0x05;
export const CMOS_RTC_DAY_WEEK = 0x06;
export const CMOS_RTC_DAY_MONTH = 0x07;
export const CMOS_RTC_MONTH = 0x08;
export const CMOS_RTC_YEAR = 0x09;
export const CMOS_STATUS_A = 0x0a;
export const CMOS_STATUS_B = 0x0b;
export const CMOS_STATUS_C = 0x0c;
export const CMOS_STATUS_D = 0x0d;
export const CMOS_DIAG_STATUS = 0x0e;
export const CMOS_RESET_CODE = 0x0f;

export const CMOS_FLOPPY_DRIVE_TYPE = 0x10;
export const CMOS_DISK_DATA = 0x12;
export const CMOS_EQUIPMENT_INFO = 0x14;
export const CMOS_MEM_BASE_LOW = 0x15;
export const CMOS_MEM_BASE_HIGH = 0x16;
export const CMOS_MEM_OLD_EXT_LOW = 0x17;
export const CMOS_MEM_OLD_EXT_HIGH = 0x18;
export const CMOS_DISK_DRIVE1_TYPE = 0x19;
export const CMOS_DISK_DRIVE2_TYPE = 0x1a;
export const CMOS_DISK_DRIVE1_CYL = 0x1b;
export const CMOS_DISK_DRIVE2_CYL = 0x24;
export const CMOS_MEM_EXTMEM_LOW = 0x30;
export const CMOS_MEM_EXTMEM_HIGH = 0x31;
export const CMOS_CENTURY = 0x32;
export const CMOS_MEM_EXTMEM2_LOW = 0x34;
export const CMOS_MEM_EXTMEM2_HIGH = 0x35;
export const CMOS_CENTURY2 = 0x37;
export const CMOS_BIOS_BOOTFLAG1 = 0x38;
export const CMOS_BIOS_DISKTRANSFLAG = 0x39;
export const CMOS_BIOS_BOOTFLAG2 = 0x3d;
export const CMOS_MEM_HIGHMEM_LOW = 0x5b;
export const CMOS_MEM_HIGHMEM_MID = 0x5c;
export const CMOS_MEM_HIGHMEM_HIGH = 0x5d;
export const CMOS_BIOS_SMP_COUNT = 0x5f;

// see CPU.prototype.fill_cmos
export const BOOT_ORDER_CD_FIRST = 0x123;
export const BOOT_ORDER_HD_FIRST = 0x312;
export const BOOT_ORDER_FD_FIRST = 0x321;

/**
 * RTC (real time clock) and CMOS. The device is in the core (src/rust/cpu/rtc.rs), which answers ports 0x70 and
 * 0x71 and runs its timer in the main loop. Here: cmos_read and cmos_write for the machine's setup, the ports
 * when they are reached through JavaScript, and the snapshot format.
 * @constructor
 * @param {CPU} cpu
 */
export function RTC(cpu)
{
    /** @const @type {CPU} */
    this.cpu = cpu;

    cpu.rtc_init(Date.now(), v86.microtick());

    cpu.io.register_write(0x70, this, function(out_byte) { cpu.rtc_port_write(0x70, out_byte); });
    cpu.io.register_write(0x71, this, function(out_byte) { cpu.rtc_port_write(0x71, out_byte); });
    cpu.io.register_read(0x71, this, function() { return cpu.rtc_port_read(0x71); });
}

// The device's memory in the core (struct Rtc in src/rust/cpu/rtc.rs)
RTC.prototype.fields = function()
{
    const buffer = this.cpu.wasm_memory.buffer, at = this.cpu.get_rtc_addr();
    return {
        // rtc_time, last_update, next_interrupt, next_interrupt_alarm, periodic_interrupt_time, update_interrupt_time
        times: new Float64Array(buffer, at, 6),
        cmos_data: new Uint8Array(buffer, at + 48, 128),
        // cmos_index, cmos_a, cmos_b, cmos_c, nmi_disabled, periodic_interrupt, update_interrupt, cmos_diag_status
        bytes: new Uint8Array(buffer, at + 176, 8),
    };
};

RTC.prototype.get_state = function()
{
    const f = this.fields();
    var state = [];

    state[0] = f.bytes[0];
    state[1] = new Uint8Array(f.cmos_data);
    state[2] = f.times[0];
    state[3] = f.times[1];
    state[4] = f.times[2];
    state[5] = f.times[3];
    state[6] = !!f.bytes[5];
    state[7] = f.times[4];
    state[8] = f.bytes[1];
    state[9] = f.bytes[2];
    state[10] = f.bytes[3];
    state[11] = f.bytes[4];
    state[12] = !!f.bytes[6];
    state[13] = f.times[5];
    state[14] = f.bytes[7];

    return state;
};

RTC.prototype.set_state = function(state)
{
    const f = this.fields();
    f.bytes[0] = state[0];
    f.cmos_data.set(state[1]);
    f.times[0] = state[2];
    f.times[1] = state[3];
    f.times[2] = state[4];
    f.times[3] = state[5];
    f.bytes[5] = state[6] ? 1 : 0;
    f.times[4] = state[7];
    f.bytes[1] = state[8];
    f.bytes[2] = state[9];
    f.bytes[3] = state[10];
    f.bytes[4] = state[11];
    f.bytes[6] = state[12] ? 1 : 0;
    f.times[5] = state[13] || 0;
    f.bytes[7] = state[14] || 0;
    this.cpu.timers_changed();
};

/** @return {number} milliseconds until the next interrupt */
RTC.prototype.timer = function(time, legacy_mode)
{
    return this.cpu.rtc_timer();
};

/**
 * @param {number} index
 */
RTC.prototype.cmos_read = function(index)
{
    dbg_assert(index < 128);
    return this.fields().cmos_data[index];
};

/**
 * @param {number} index
 * @param {number} value
 */
RTC.prototype.cmos_write = function(index, value)
{
    dbg_assert(index < 128);
    this.fields().cmos_data[index] = value;
};
