import { REG_EAX, REG_EBX, REG_ECX, REG_EDX, LOG_OTHER } from "./const.js";
import { dbg_log } from "./log.js";

// For Types Only
import { CPU } from "./cpu.js";
import { BusConnector } from "./bus.js";

const VMWARE_PORT = 0x5658;
const VMWARE_MAGIC = 0x564D5868;

const CMD_GETSELLENGTH = 6;
const CMD_GETNEXTPIECE = 7;
const CMD_SETSELLENGTH = 8;
const CMD_SETNEXTPIECE = 9;
const CMD_GETVERSION = 10;
const CMD_GETTIME = 23;
const CMD_ABSPOINTER_DATA = 39;
const CMD_ABSPOINTER_STATUS = 40;
const CMD_ABSPOINTER_COMMAND = 41;

const ABSPOINTER_ENABLE = 0x45414552;
const ABSPOINTER_DISABLE = 0x000000F5;
const ABSPOINTER_RELATIVE = 0x4C455252;
const ABSPOINTER_ABSOLUTE = 0x53424152;

const READ_ID = 0x3442554A;

const BUTTON_LEFT = 0x20;
const BUTTON_RIGHT = 0x10;
const BUTTON_MIDDLE = 0x08;

// Flag in the status dword marking a packet whose x/y are signed deltas
// rather than absolute positions (VMMOUSE_RELATIVE_PACKET in the guest
// drivers). Used while the host pointer is locked and no meaningful absolute
// position exists.
const RELATIVE_PACKET = 0x00010000;

const QUEUE_MAX = 1024;
const CLIP_MAX = 0x10000;

/**
 * VMware backdoor (port 0x5658). Lets a guest driver read absolute pointer
 * position so the guest cursor can track the host cursor 1:1 without pointer
 * lock, lets a guest agent sync the clipboard with the host using the legacy
 * text commands (6–9), and implements GETTIME (23) so a guest agent can keep
 * the guest clock in sync with the host (guests only read the RTC at boot, so
 * a restored state resumes with a stale clock). PS/2 still supplies the mouse
 * IRQ; the driver reads this port on each IRQ12. While the host pointer is
 * locked
 * (e.g. for games), movement is reported as relative packets instead, since
 * no meaningful absolute position exists.
 *
 * @constructor
 * @param {CPU} cpu
 * @param {BusConnector} bus
 */
export function VMwareMouse(cpu, bus)
{
    /** @const @type {CPU} */
    this.cpu = cpu;
    /** @const @type {BusConnector} */
    this.bus = bus;

    // The pointer (its commands and packet queue) is the core's: src/rust/cpu/vmware.rs, which answers its port.
    const core = cpu.wm.exports;
    this.core = core;
    core["vmware_init"]();

    /** @type {Uint8Array} host→guest text staged for the guest to read */
    this.clip_out = new Uint8Array(0);
    this.clip_out_cursor = 0;
    this.clip_out_fresh = false;
    /** @type {Uint8Array} guest→host text being received */
    this.clip_in = new Uint8Array(0);
    this.clip_in_cursor = 0;

    this.bus.register("vmware-clipboard-host", function(data)
    {
        this.clip_out = data.length > CLIP_MAX ? data.subarray(0, CLIP_MAX) : data;
        this.clip_out_cursor = 0;
        this.clip_out_fresh = true;
    }, this);

    /**
     * Whether the host pointer is currently locked (browser pointer lock).
     * While locked, the host cursor position is meaningless, so movement is
     * reported as relative packets instead of absolute ones.
     * @type {boolean}
     */
    this.host_pointer_locked = false;
    this.rel_dx = 0;
    this.rel_dy = 0;

    this.bus.register("mouse-absolute", function(data)
    {
        const x = Math.max(0, Math.min(0xFFFF, Math.round(data[0] / data[2] * 0xFFFF)));
        const y = Math.max(0, Math.min(0xFFFF, Math.round(data[1] / data[3] * 0xFFFF)));
        core["vmware_move"](x, y);
    }, this);

    this.bus.register("mouse-delta", function(data)
    {
        if(!this.host_pointer_locked)
        {
            return;
        }
        this.rel_dx += data[0];
        this.rel_dy += data[1];
        const dx = this.rel_dx | 0;
        const dy = this.rel_dy | 0;
        if(!dx && !dy)
        {
            return;
        }
        this.rel_dx -= dx;
        this.rel_dy -= dy;
        core["vmware_push_relative"](dx, dy, 0, true);
    }, this);

    this.bus.register("mouse-pointer-lock", function(locked)
    {
        this.host_pointer_locked = locked;
        this.rel_dx = 0;
        this.rel_dy = 0;
    }, this);

    this.bus.register("mouse-click", function(data)
    {
        core["vmware_buttons"](
            (data[0] ? BUTTON_LEFT : 0) |
            (data[1] ? BUTTON_MIDDLE : 0) |
            (data[2] ? BUTTON_RIGHT : 0));
        if(this.host_pointer_locked)
        {
            core["vmware_push_relative"](0, 0, 0, false);
        }
        else
        {
            core["vmware_push_absolute"](0, false);
        }
    }, this);

    this.bus.register("mouse-wheel", function(data)
    {
        if(this.host_pointer_locked)
        {
            core["vmware_push_relative"](0, 0, -data[0] | 0, false);
        }
        else
        {
            core["vmware_push_absolute"](-data[0] | 0, false);
        }
    }, this);

    const nop = function() {};
    cpu.io.register_read(VMWARE_PORT, this,
        function() { return 0xFF; }, function() { return 0xFFFF; }, this.port_read32);
    cpu.io.register_write(VMWARE_PORT, this, nop, nop, nop);
}

VMwareMouse.prototype.port_read32 = function()
{
    const reg32 = this.cpu.reg32;
    if(reg32[REG_EAX] !== VMWARE_MAGIC)
    {
        return 0xFFFFFFFF | 0;
    }

    switch(reg32[REG_ECX] & 0xFFFF)
    {
        case CMD_GETVERSION:
        case CMD_ABSPOINTER_STATUS:
        case CMD_ABSPOINTER_DATA:
        case CMD_ABSPOINTER_COMMAND:
            // (the pointer's: the core answers these, also when they come through JavaScript)
            return this.core["vmware_read32"]();

        case CMD_GETSELLENGTH:
            if(!this.clip_out_fresh)
            {
                return 0xFFFFFFFF | 0;
            }
            this.clip_out_fresh = false;
            this.clip_out_cursor = 0;
            return this.clip_out.length;

        case CMD_GETNEXTPIECE:
        {
            const c = this.clip_out;
            let i = this.clip_out_cursor;
            const v = (c[i] | 0) | (c[i + 1] | 0) << 8 |
                      (c[i + 2] | 0) << 16 | (c[i + 3] | 0) << 24;
            this.clip_out_cursor = i + 4;
            return v;
        }

        case CMD_SETSELLENGTH:
        {
            const n = Math.min(reg32[REG_EBX] >>> 0, CLIP_MAX);
            this.clip_in = new Uint8Array(n);
            this.clip_in_cursor = 0;
            if(n === 0)
            {
                this.bus.send("vmware-clipboard-guest", this.clip_in);
            }
            return 0;
        }

        case CMD_SETNEXTPIECE:
        {
            const c = this.clip_in;
            const v = reg32[REG_EBX] >>> 0;
            let i = this.clip_in_cursor;
            if(i < c.length) c[i++] = v;
            if(i < c.length) c[i++] = v >>> 8;
            if(i < c.length) c[i++] = v >>> 16;
            if(i < c.length) c[i++] = v >>> 24;
            this.clip_in_cursor = i;
            if(i >= c.length)
            {
                this.bus.send("vmware-clipboard-guest", c);
                this.clip_in = new Uint8Array(0);
                this.clip_in_cursor = 0;
            }
            return 0;
        }

        case CMD_GETTIME:
        {
            // EAX = host time in seconds since the Unix epoch (UTC),
            // EBX = remaining microseconds, ECX = maximum time lag in
            // microseconds, EDX = host's offset from UTC in minutes (east
            // positive). Deprecated upstream in favour of GETTIMEFULL because
            // EAX overflows as a signed value in 2038, but it's the simplest
            // command a 32-bit guest agent can consume — read unsigned it's
            // good until 2106.
            const now = Date.now();
            reg32[REG_EBX] = now % 1000 * 1000;
            reg32[REG_ECX] = 1000000;
            reg32[REG_EDX] = -new Date(now).getTimezoneOffset();
            return now / 1000 >>> 0;
        }

    }
    return 0xFFFFFFFF | 0;
};

VMwareMouse.prototype.get_state = function()
{
    return [this.core["vmware_state"](0), this.core["vmware_state"](1)];
};

VMwareMouse.prototype.set_state = function(state)
{
    this.core["vmware_set_state"](!!state[0], !!state[1]);
    this.bus.send("vmware-absolute-mouse", !!state[1]);
};
