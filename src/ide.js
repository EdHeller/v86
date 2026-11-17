import { LOG_DISK } from "./const.js";
import { h } from "./lib.js";
import { dbg_assert, dbg_log } from "./log.js";
import { CMOS_BIOS_DISKTRANSFLAG, CMOS_DISK_DATA, CMOS_DISK_DRIVE1_CYL, CMOS_DISK_DRIVE1_TYPE, CMOS_DISK_DRIVE2_CYL, CMOS_DISK_DRIVE2_TYPE } from "./rtc.js";

// For Types Only
import { CPU } from "./cpu.js";
import { BusConnector } from "./bus.js";

// ATA/ATAPI-6/8 IDE Controller
//
// References
// - [ATA8-ACS]
//   ATA/ATAPI Command Set - 3 (ACS-3) (Rev. 5, Oct. 28, 2013)
//   https://read.seas.harvard.edu/cs161/2019/pdf/ata-atapi-8.pdf
// - [ATA-6]
//   AT Attachment with Packet Interface - 6 (ATA/ATAPI-6) (Rev. 3a; Dec. 14, 2001)
//   https://technion-csl.github.io/ose/readings/hardware/ATA-d1410r3a.pdf
// - [CD-SCSI-2]
//   PROPOSAL FOR CD-ROM IN SCSI-2 (X3T9.2/87) (Rev. 0, Jun. 30, 1987)
//   https://www.t10.org/ftp/x3t9.2/document.87/87-106r0.txt
//   https://www.t10.org/ftp/x3t9.2/document.87/87-106r1.txt (errata to r0)
// - [SAM-3]
//   SCSI Architecture Model - 3 (SAM-3) (Sep. 21, 2004)
//   https://dn790004.ca.archive.org/0/items/SCSISpecificationDocumentsSCSIDocuments/SCSI%20Architecture%20Model/SCSI%20Architecture%20Model%203%20rev%2014.pdf
// - [SPC-3]
//   SCSI Primary Commands - 3 (SPC-3) (July 20, 2008)
//   https://www.t10.org/ftp/t10/document.08/08-309r0.pdf
// - [MMC-3]
//   SCSI Multimedia Commands - 3 (MMC-3) (Rev. 10g, Nov. 12, 2001)
//   https://ia902808.us.archive.org/33/items/mmc3r10g/mmc3r10g.pdf
// - [MMC-2]
//   Packet Commands for C/DVD Devices (1997)
//   https://www.t10.org/ftp/t10/document.97/97-108r0.pdf
// - [BMI-1]
//   Programming Interface for Bus Master IDE Controller, Revision 1.0, 5/16/94
//   https://storage.googleapis.com/google-code-archive-downloads/v2/code.google.com/u9proj/idems100.pdf
// - [SFF-8020]
//   ATA Packet Interface for CD-ROMs (Rev. 1.2, Feb. 12, 1994)
//   https://dn790009.ca.archive.org/0/items/SCSISpecificationDocumentsATAATAPI/SFF-8020_%20ATA%20Packet%20Interface%20for%20CD-ROMs%20-%20SFF.pdf
// - [AACS]
//   Advanced Access Control System (AACS) (Rev. 0.953, Oct. 26, 2012)
//   https://aacsla.com/wp-content/uploads/2019/02/AACS_Spec_Common_Final_0953.pdf

const CDROM_SECTOR_SIZE = 2048;
const HD_SECTOR_SIZE = 512;

const BUS_MASTER_BASE = 0xB400;

// Per-channel ATA register offsets, legend:
//   (*1*) Control block register (BAR1/3), else: Command block register (BAR0/2)
// Read-only registers:
const ATA_REG_ERROR      = 0x01;  // Error register, see [ATA-6] 7.9
const ATA_REG_STATUS     = 0x07;  // Status register, see [ATA-6] 7.15
const ATA_REG_ALT_STATUS = 0x00;  // (*1*) Alternate Status register, see [ATA-6] 7.3
// Read-/writable registers:
const ATA_REG_DATA       = 0x00;  // Data register, see [ATA-6] 7.6
const ATA_REG_SECTOR     = 0x02;  // Sector Count register, see [ATA-6] 7.14
const ATA_REG_LBA_LOW    = 0x03;  // LBA Low register, see [ATA-6] 7.12
const ATA_REG_LBA_MID    = 0x04;  // LBA Mid register, see [ATA-6] 7.13
const ATA_REG_LBA_HIGH   = 0x05;  // LBA High register, see [ATA-6] 7.11
const ATA_REG_DEVICE     = 0x06;  // Device register, see [ATA-6] 7.7
// Write-only registers:
const ATA_REG_FEATURES   = 0x01;  // Features register, see [ATA-6] 7.10
const ATA_REG_COMMAND    = 0x07;  // Command register, see [ATA-6] 7.4
const ATA_REG_CONTROL    = 0x00;  // (*1*) Device Control register, see [ATA-6] 7.8

// Per-channel Bus Master IDE register offsets (BAR4), see [BMI-1] 2.0
// these are the primary channel's offsets, add 8 for secondary
const BMI_REG_COMMAND = 0x00;     // Bus Master IDE Command register
const BMI_REG_STATUS = 0x02;      // Bus Master IDE Status register
const BMI_REG_PRDT = 0x04;        // Bus Master IDE PRD Table Address register

// Error register bits:
// All bits except for bit 0x04 are command dependent.
const ATA_ER_ABRT = 0x04;  // Command aborted

// Status register bits:
const ATA_SR_ERR  = 0x01;  // Error (ATA)
const ATA_SR_COND = 0x01;  // Check Condition (ATAPI)
const ATA_SR_SENS = 0x02;  // Sense Available (ATAPI)
const ATA_SR_AERR = 0x04;  // Alignment Error
const ATA_SR_DRQ  = 0x08;  // Data Request
const ATA_SR_DSC  = 0x10;  // Drive Seek Complete / Deferred Write Error
const ATA_SR_DF   = 0x20;  // Device Fault / Stream Error
const ATA_SR_DRDY = 0x40;  // Drive Ready
const ATA_SR_BSY  = 0x80;  // Busy

// Device register bits:
// Bits 0x20/0x80 are obsolete and 0x01/0x02/0x04/0x08/0x40 are command dependent.
const ATA_DR_DEV = 0x10;   // Device select; slave device if set, else master device

// Device Control register bits:
// Bits 0x08/0x10/0x20/0x40 are reserved and bit 0x01 is always zero.
const ATA_CR_NIEN = 0x02;  // Interrupt disable (not Interrupt ENable)
const ATA_CR_SRST = 0x04;  // Software reset
const ATA_CR_HOB = 0x80;   // 48-bit Address feature set

// ATA commands
const ATA_CMD_DEVICE_RESET = 0x08;                    // see [ATA8-ACS] 7.6
const ATA_CMD_EXECUTE_DEVICE_DIAGNOSTIC = 0x90;       // see [ATA8-ACS] 7.9
const ATA_CMD_FLUSH_CACHE = 0xE7;                     // see [ATA8-ACS] 7.10
const ATA_CMD_FLUSH_CACHE_EXT = 0xEA;                 // see [ATA8-ACS] 7.11
const ATA_CMD_GET_MEDIA_STATUS = 0xDA;                // see [ATA-6] 8.14
const ATA_CMD_IDENTIFY_DEVICE = 0xEC;                 // see [ATA8-ACS] 7.12
const ATA_CMD_IDENTIFY_PACKET_DEVICE = 0xA1;          // see [ATA8-ACS] 7.13
const ATA_CMD_IDLE_IMMEDIATE = 0xE1;                  // see [ATA8-ACS] 7.15
const ATA_CMD_INITIALIZE_DEVICE_PARAMETERS = 0x91;    // not mentioned in [ATA-6] or [ATA8-ACS]
const ATA_CMD_MEDIA_LOCK = 0xDE;                      // see [ATA-6] 8.20
const ATA_CMD_NOP = 0x00;                             // see [ATA8-ACS] 7.17
const ATA_CMD_PACKET = 0xA0;                          // see [ATA8-ACS] 7.18
const ATA_CMD_READ_DMA = 0xC8;                        // see [ATA8-ACS] 7.21
const ATA_CMD_READ_DMA_EXT = 0x25;                    // see [ATA8-ACS] 7.22
const ATA_CMD_READ_MULTIPLE = 0x29;                   // see [ATA8-ACS] 7.26
const ATA_CMD_READ_MULTIPLE_EXT = 0xC4;               // see [ATA8-ACS] 7.27
const ATA_CMD_READ_NATIVE_MAX_ADDRESS = 0xF8;         // see [ATA-6] 8.32
const ATA_CMD_READ_NATIVE_MAX_ADDRESS_EXT = 0x27;     // see [ATA-6] 8.33
const ATA_CMD_READ_SECTORS = 0x20;                    // see [ATA8-ACS] 7.28
const ATA_CMD_READ_SECTORS_EXT = 0x24;                // see [ATA8-ACS] 7.29
const ATA_CMD_READ_VERIFY_SECTORS = 0x40;             // see [ATA8-ACS] 7.32
const ATA_CMD_SECURITY_FREEZE_LOCK = 0xF5;            // see [ATA8-ACS] 7.40
const ATA_CMD_SET_FEATURES = 0xEF;                    // see [ATA8-ACS] 7.45
const ATA_CMD_SET_MAX = 0xF9;                         // see [ATA-6] 8.47
const ATA_CMD_SET_MULTIPLE_MODE = 0xC6;               // see [ATA8-ACS] 7.46
const ATA_CMD_STANDBY_IMMEDIATE = 0xE0;               // see [ATA8-ACS] 7.50
const ATA_CMD_WRITE_DMA = 0xCA;                       // see [ATA8-ACS] 7.58
const ATA_CMD_WRITE_DMA_EXT = 0x35;                   // see [ATA8-ACS] 7.59
const ATA_CMD_WRITE_MULTIPLE = 0x39;                  // see [ATA8-ACS] 7.64
const ATA_CMD_WRITE_MULTIPLE_EXT = 0xC5;              // see [ATA8-ACS] 7.65
const ATA_CMD_WRITE_SECTORS = 0x30;                   // see [ATA8-ACS] 7.67
const ATA_CMD_WRITE_SECTORS_EXT = 0x34;               // see [ATA8-ACS] 7.68
const ATA_CMD_READ_LOG_EXT = 0x2F;                    // see [ACS-3] 7.42
const ATA_CMD_10h = 0x10;                             // command obsolete/unknown, see [ATA-6] Table E.2
const ATA_CMD_F0h = 0xF0;                             // vendor-specific

const ATA_CMD_NAME =
{
    [ATA_CMD_DEVICE_RESET]:                 "DEVICE RESET",
    [ATA_CMD_EXECUTE_DEVICE_DIAGNOSTIC]:    "EXECUTE DEVICE DIAGNOSTIC",
    [ATA_CMD_FLUSH_CACHE]:                  "FLUSH CACHE",
    [ATA_CMD_FLUSH_CACHE_EXT]:              "FLUSH CACHE EXT",
    [ATA_CMD_GET_MEDIA_STATUS]:             "GET MEDIA STATUS",
    [ATA_CMD_IDENTIFY_DEVICE]:              "IDENTIFY DEVICE",
    [ATA_CMD_IDENTIFY_PACKET_DEVICE]:       "IDENTIFY PACKET DEVICE",
    [ATA_CMD_IDLE_IMMEDIATE]:               "IDLE IMMEDIATE",
    [ATA_CMD_INITIALIZE_DEVICE_PARAMETERS]: "INITIALIZE DEVICE PARAMETERS",
    [ATA_CMD_MEDIA_LOCK]:                   "MEDIA LOCK",
    [ATA_CMD_NOP]:                          "NOP",
    [ATA_CMD_PACKET]:                       "PACKET",
    [ATA_CMD_READ_DMA]:                     "READ DMA",
    [ATA_CMD_READ_DMA_EXT]:                 "READ DMA EXT",
    [ATA_CMD_READ_MULTIPLE]:                "READ MULTIPLE",
    [ATA_CMD_READ_MULTIPLE_EXT]:            "READ MULTIPLE EXT",
    [ATA_CMD_READ_NATIVE_MAX_ADDRESS]:      "READ NATIVE MAX ADDRESS",
    [ATA_CMD_READ_NATIVE_MAX_ADDRESS_EXT]:  "READ NATIVE MAX ADDRESS EXT",
    [ATA_CMD_READ_SECTORS]:                 "READ SECTORS",
    [ATA_CMD_READ_SECTORS_EXT]:             "READ SECTORS EXT",
    [ATA_CMD_READ_VERIFY_SECTORS]:          "READ VERIFY SECTORS",
    [ATA_CMD_SECURITY_FREEZE_LOCK]:         "SECURITY FREEZE LOCK",
    [ATA_CMD_SET_FEATURES]:                 "SET FEATURES",
    [ATA_CMD_SET_MAX]:                      "SET MAX",
    [ATA_CMD_SET_MULTIPLE_MODE]:            "SET MULTIPLE MODE",
    [ATA_CMD_STANDBY_IMMEDIATE]:            "STANDBY IMMEDIATE",
    [ATA_CMD_WRITE_DMA]:                    "WRITE DMA",
    [ATA_CMD_WRITE_DMA_EXT]:                "WRITE DMA EXT",
    [ATA_CMD_WRITE_MULTIPLE]:               "WRITE MULTIPLE",
    [ATA_CMD_WRITE_MULTIPLE_EXT]:           "WRITE MULTIPLE EXT",
    [ATA_CMD_WRITE_SECTORS]:                "WRITE SECTORS",
    [ATA_CMD_WRITE_SECTORS_EXT]:            "WRITE SECTORS EXT",
    [ATA_CMD_READ_LOG_EXT]:                 "READ LOG EXT",
    [ATA_CMD_10h]:                          "<UNKNOWN 10h>",
    [ATA_CMD_F0h]:                          "<VENDOR-SPECIFIC F0h>",
};

// ATAPI (SCSI-2/MMC-2) commands
const ATAPI_CMD_GET_CONFIGURATION = 0x46;             // see [CD-SCSI-2]
const ATAPI_CMD_GET_EVENT_STATUS_NOTIFICATION = 0x4A; // see [MMC-2] 9.1.2
const ATAPI_CMD_INQUIRY = 0x12;                       // see [MMC-2] 9.1.3
const ATAPI_CMD_MECHANISM_STATUS = 0xBD;              // see [MMC-2] 9.1.5
const ATAPI_CMD_MODE_SENSE_6 = 0x1A;                  // see [CD-SCSI-2]
const ATAPI_CMD_MODE_SENSE_10 = 0x5A;                 // see [MMC-2] 9.1.7
const ATAPI_CMD_PAUSE = 0x45;                         // see [CD-SCSI-2]
const ATAPI_CMD_PREVENT_ALLOW_MEDIUM_REMOVAL = 0x1E;  // see [MMC-2] 9.1.9
const ATAPI_CMD_READ_10 = 0x28;                       // see [CD-SCSI-2]
const ATAPI_CMD_READ_12 = 0xA8;                       // see [SFF-8020] 9.8.14
const ATAPI_CMD_READ_CAPACITY = 0x25;                 // see [MMC-2] 9.1.12
const ATAPI_CMD_READ_FORMAT_CAPACITIES = 0x23;        // see [MMC-3] 5.21
const ATAPI_CMD_READ_CD = 0xBE;                       // see [CD-SCSI-2]
const ATAPI_CMD_READ_DISK_INFORMATION = 0x51;         // see [CD-SCSI-2]
const ATAPI_CMD_READ_SUBCHANNEL = 0x42;               // see [CD-SCSI-2]
const ATAPI_CMD_READ_TOC_PMA_ATIP = 0x43;             // see [CD-SCSI-2]
const ATAPI_CMD_READ_TRACK_INFORMATION = 0x52;        // see [CD-SCSI-2]
const ATAPI_CMD_REQUEST_SENSE = 0x03;                 // see [MMC-2] 9.1.18
const ATAPI_CMD_START_STOP_UNIT = 0x1B;               // see [CD-SCSI-2]
const ATAPI_CMD_TEST_UNIT_READY = 0x00;               // see [MMC-2] 9.1.20
const ATAPI_CMD_REPORT_KEY = 0xA4;                    // see [MMC-3] 5.26
const ATAPI_CMD_READ_DISC_STRUCTURE = 0xAD;           // see [AASC] 4.14.3

// ATAPI command flags
const ATAPI_CF_NONE = 0x00;         // no flags
const ATAPI_CF_NEEDS_DISK = 0x01;   // command needs inserted disk
const ATAPI_CF_UNIT_ATTN = 0x02;    // bounce command if unit attention condition is active

// ATAPI commands, for flags see [MMC-3] 4.2.6
const ATAPI_CMD =
{
    [ATAPI_CMD_GET_CONFIGURATION]:             {name: "GET CONFIGURATION",             flags: ATAPI_CF_NONE},
    [ATAPI_CMD_GET_EVENT_STATUS_NOTIFICATION]: {name: "GET EVENT STATUS NOTIFICATION", flags: ATAPI_CF_NONE},
    [ATAPI_CMD_INQUIRY]:                       {name: "INQUIRY",                       flags: ATAPI_CF_NONE},
    [ATAPI_CMD_MECHANISM_STATUS]:              {name: "MECHANISM STATUS",              flags: ATAPI_CF_NONE},
    [ATAPI_CMD_MODE_SENSE_6]:                  {name: "MODE SENSE (6)",                flags: ATAPI_CF_NONE},
    [ATAPI_CMD_MODE_SENSE_10]:                 {name: "MODE SENSE (10)",               flags: ATAPI_CF_NONE},
    [ATAPI_CMD_PAUSE]:                         {name: "PAUSE",                         flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_PREVENT_ALLOW_MEDIUM_REMOVAL]:  {name: "PREVENT ALLOW MEDIUM REMOVAL",  flags: ATAPI_CF_NONE},
    [ATAPI_CMD_READ_10]:                       {name: "READ (10)",                     flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_12]:                       {name: "READ (12)",                     flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_CAPACITY]:                 {name: "READ CAPACITY",                 flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_FORMAT_CAPACITIES]:        {name: "READ FORMAT CAPACITIES",        flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_CD]:                       {name: "READ CD",                       flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_DISK_INFORMATION]:         {name: "READ DISK INFORMATION",         flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_SUBCHANNEL]:               {name: "READ SUBCHANNEL",               flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_TOC_PMA_ATIP]:             {name: "READ TOC PMA ATIP",             flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_READ_TRACK_INFORMATION]:        {name: "READ TRACK INFORMATION",        flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_REQUEST_SENSE]:                 {name: "REQUEST SENSE",                 flags: ATAPI_CF_NONE},
    [ATAPI_CMD_START_STOP_UNIT]:               {name: "START STOP UNIT",               flags: ATAPI_CF_NONE},
    [ATAPI_CMD_TEST_UNIT_READY]:               {name: "TEST UNIT READY",               flags: ATAPI_CF_NEEDS_DISK},
    [ATAPI_CMD_REPORT_KEY]:                    {name: "REPORT KEY",                    flags: ATAPI_CF_NONE},
    [ATAPI_CMD_READ_DISC_STRUCTURE]:           {name: "READ DISC STRUCTURE",           flags: ATAPI_CF_NEEDS_DISK},
};

// ATAPI device signature
const ATAPI_SIGNATURE_LO = 0x14;
const ATAPI_SIGNATURE_HI = 0xEB;

// ATAPI 4-bit Sense Keys, see [MMC-2] 9.1.18.3, Table 123
const ATAPI_SK_NO_SENSE = 0;
const ATAPI_SK_RECOVERED_ERROR = 1;
const ATAPI_SK_NOT_READY = 2;
const ATAPI_SK_MEDIUM_ERROR = 3;
const ATAPI_SK_HARDWARE_ERROR = 4;
const ATAPI_SK_ILLEGAL_REQUEST = 5;
const ATAPI_SK_UNIT_ATTENTION = 6;
const ATAPI_SK_DATA_PROTECT = 7;
const ATAPI_SK_BLANK_CHECK = 8;
const ATAPI_SK_ABORTED_COMMAND = 11;

// ATAPI 8-bit Additional Sense Codes, see [MMC-2] 9.1.18.3, Table 124
// https://github.com/qemu/qemu/blob/3c5a5e213e5f08fbfe70728237f7799ac70f5b99/hw/ide/ide-internal.h#L288
const ATAPI_ASC_INV_FIELD_IN_CMD_PACKET = 0x24;
const ATAPI_ASC_MEDIUM_MAY_HAVE_CHANGED = 0x28;
const ATAPI_ASC_MEDIUM_NOT_PRESENT = 0x3A;

// Pending media-change state, reported to the guest in order on a swap.
const MEDIUM_CHANGED_NONE = 0;
const MEDIUM_CHANGED_UNIT_ATTENTION = 1;
const MEDIUM_CHANGED_NOT_PRESENT = 2;

// Debug log detail bits (internal to this module)
const LOG_DETAIL_NONE = 0x00;   // disable debug logging of details
const LOG_DETAIL_REG_IO = 0x01; // log register read/write access
const LOG_DETAIL_IRQ = 0x02;    // log IRQ raise/lower events
const LOG_DETAIL_RW = 0x04;     // log data read/write-related events
const LOG_DETAIL_RW_DMA = 0x08; // log DMA data read/write-related events
const LOG_DETAIL_CHS = 0x10;    // log register-CHS to LBA conversions
const LOG_DETAIL_ALL = 0xFF;    // log all details
// the bitset of active log details (should be 0 when not in DEBUG mode)
const LOG_DETAILS = DEBUG ? LOG_DETAIL_NONE : 0;

/**
 * @constructor
 * @param {CPU} cpu
 * @param {BusConnector} bus
 *
 * ide_config: [ [primary-master, primary-slave], [secondary-master, secondary-slave] ]
 *
 *   Each of the four arguments (primary-master, primary-slave, ...) is either
 *   undefined or an object of the form:
 *
 *       { buffer: Uint8Array, is_cdrom: bool }
 *
 *   If is_cdrom is defined and true:
 *   - If buffer is defined: create an ATAPI CD-ROM device using buffer as inserted disk
 *   - If buffer is undefined: create an ATAPI CD-ROM device with ejectd disk
 *   If is_cdrom is undefined or false:
 *   - If buffer is defined: create an ATA Hard-Disk device using buffer as disk image
 *   - If buffer is undefined: represents a missing device
 *
 *   A slave drive can only exist if a master drive also exists.
 * */
export function IDEController(cpu, bus, ide_config)
{
    this.cpu = cpu;
    this.bus = bus;

    this.primary = undefined;
    this.secondary = undefined;

    const has_primary = ide_config && ide_config[0][0];
    const has_secondary = ide_config && ide_config[1][0];
    if(has_primary || has_secondary)
    {
        if(has_primary)
        {
            this.primary = new IDEChannel(this, 0, ide_config[0], 0x1F0, 0x3F6, 14);
        }
        if(has_secondary)
        {
            this.secondary = new IDEChannel(this, 1, ide_config[1], 0x170, 0x376, 15);
        }

        const vendor_id = 0x8086;    // Intel Corporation
        const device_id = 0x7010;    // 82371SB PIIX3 IDE [Natoma/Triton II]
        const class_code = 0x01;     // Mass Storage Controller
        const subclass = 0x01;       // IDE Controller
        const prog_if = 0x80;        // ISA Compatibility mode-only controller, supports bus mastering
        const interrupt_line = 0x00; // IRQs 14 and 15 are predefined in Compatibility mode and this field is ignored
        const command_base0 = has_primary ? this.primary.command_base : 0;
        const control_base0 = has_primary ? this.primary.control_base : 0;
        const command_base1 = has_secondary ? this.secondary.command_base : 0;
        const control_base1 = has_secondary ? this.secondary.control_base : 0;

        this.name = "ide";
        this.pci_id = 0x1E << 3;
        // (the bus master registers: answered by the core, at BUS_MASTER_BASE and where the BIOS moves BAR4)
        this.cpu.ide_set_bus_master(BUS_MASTER_BASE, this.pci_id);
        this.pci_space = [
            vendor_id & 0xFF, vendor_id >> 8, device_id & 0xFF, device_id >> 8, 0x05, 0x00, 0xA0, 0x02,
            0x00, prog_if, subclass, class_code, 0x00, 0x00, 0x00, 0x00,
            command_base0 & 0xFF   | 1, command_base0 >> 8,   0x00, 0x00,
            control_base0 & 0xFF   | 1, control_base0 >> 8,   0x00, 0x00,
            command_base1 & 0xFF   | 1, command_base1 >> 8,   0x00, 0x00,
            control_base1 & 0xFF   | 1, control_base1 >> 8,   0x00, 0x00,
            BUS_MASTER_BASE & 0xFF | 1, BUS_MASTER_BASE >> 8, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
            0x43, 0x10, 0xD4, 0x82,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, interrupt_line, 0x01, 0x00, 0x00,
            // 0x40: IDETIM, both channels' decode enabled (as PIIX3 resets it)
            0x00, 0x80, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            // 0x80
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        this.pci_bars = [
            has_primary ? { size: 8 } : undefined,   // BAR0: Command block register address of primary channel
            has_primary ? { size: 1 } : undefined,   // BAR1: Control block register address of primary channel
            has_secondary ? { size: 8 } : undefined, // BAR2: Command block register address of secondary channel
            has_secondary ? { size: 1 } : undefined, // BAR3: Control block register address of secondary channel
            { size: 16 }                             // BAR4: Bus Master I/O register address of both channels (8+8)
        ];
        cpu.devices.pci.register_device(this);
    }

    Object.seal(this);
}

IDEController.prototype.get_state = function()
{
    const state = [];
    state[0] = this.primary;
    state[1] = this.secondary;
    return state;
};

IDEController.prototype.set_state = function(state)
{
    this.primary && this.primary.set_state(state[0]);
    this.secondary && this.secondary.set_state(state[1]);
};

/** a 32-bit field of a device in the core's memory (made at each use: the memory can grow) */
function core_i32(cpu, address, index)
{
    return new Int32Array(cpu.wasm_memory.buffer, address, index + 1)[index];
}

function set_core_i32(cpu, address, index, value)
{
    new Int32Array(cpu.wasm_memory.buffer, address, index + 1)[index] = value;
}

/**
 * Turns fields of an object into accessors of 32-bit fields of a device in the core's memory
 * @param {!Object} object
 * @param {?} cpu
 * @param {function():number} address
 * @param {!Object<string, number>} fields name to index
 */
function define_core_fields(object, cpu, address, fields)
{
    for(const name of Object.keys(fields))
    {
        const index = fields[name];
        Object.defineProperty(object, name, {
            get: () => core_i32(cpu, address(), index),
            set: value => { set_core_i32(cpu, address(), index, value); },
        });
    }
}

// The registers of an interface in struct Iface (src/rust/cpu/ide.rs)
const IFACE_FIELDS = {
    "status_reg": 0, "error_reg": 1, "sector_count_reg": 2, "lba_low_reg": 3, "lba_mid_reg": 4, "lba_high_reg": 5,
    "device_reg": 6, "features_reg": 7, "is_lba": 8, "head": 9, "drive_connected": 10,
};

// The drives' fields in the core (struct DriveFields in src/rust/cpu/ide.rs): 32-bit ones by index, then doubles
const DRIVE_FIELDS = {
    "is_atapi": 0, "has_disk_": 1, "sector_size": 2, "sectors_per_drq": 3, "head_count": 4, "sectors_per_track": 5,
    "cylinder_count": 6, "data_pointer": 7, "data_length": 8, "data_end": 9, "current_command": 10,
    "current_atapi_command": 11, "atapi_sense_key": 12, "atapi_add_sense": 13, "medium_changed": 14,
};
const DRIVE_DOUBLES = { "sector_count": 64, "write_dest": 72, "disk_bytes": 80 };

/**
 * @constructor
 * @param {IDEController} controller
 * @param {number} channel_nr
 * */
function IDEChannel(controller, channel_nr, channel_config, command_base, control_base, irq)
{
    this.controller = controller;
    this.channel_nr = channel_nr;
    this.cpu = controller.cpu;
    this.bus = controller.bus;
    this.command_base = command_base;
    this.control_base = control_base;
    this.irq = irq;
    this.name = "ide" + channel_nr;

    // The channel, its registers and its drives' commands, data transfers and DMA are in the core
    // (src/rust/cpu/ide.rs), which answers the ports; here: the disk images (reads and writes the core asks for),
    // the drives' geometry in the CMOS, inserting and ejecting a CD, and the snapshot format.
    const core = this.cpu;
    core.ide_channel_init(channel_nr, command_base, control_base, irq);
    define_core_fields(this, core, () => core.get_ide_addr(channel_nr),
        { "device_control_reg": 23, "prdt_addr": 28, "dma_status": 29, "dma_command": 30 });
    Object.defineProperty(this, "current_interface", {
        get: () => core_i32(core, core.get_ide_addr(channel_nr), 22) ? this.slave : this.master,
        set: iface => { set_core_i32(core, core.get_ide_addr(channel_nr), 22, iface === this.slave ? 1 : 0); },
    });

    const master_cfg = channel_config ? channel_config[0] : undefined;
    const slave_cfg = channel_config ? channel_config[1] : undefined;
    this.master = new IDEInterface(this, 0, master_cfg?.buffer, master_cfg?.is_cdrom);
    this.slave = new IDEInterface(this, 1, slave_cfg?.buffer, slave_cfg?.is_cdrom);

    this.current_interface = this.master;
    this.device_control_reg = ATA_CR_NIEN;
    this.prdt_addr = 0;
    this.dma_status = 0;
    this.dma_command = 0;

    // the ports, when they are reached through JavaScript (the core answers them itself)
    const exports = core.wm.exports;
    const read = port => () => exports["ide_core_read"](port, 8);
    const write = port => value => exports["ide_core_write"](port, 8, value);
    const io = core.io;
    io.register_read(this.command_base, this,
        () => exports["ide_core_read"](this.command_base, 8),
        () => exports["ide_core_read"](this.command_base, 16),
        () => exports["ide_core_read"](this.command_base, 32));
    io.register_write(this.command_base, this,
        value => exports["ide_core_write"](this.command_base, 8, value),
        value => exports["ide_core_write"](this.command_base, 16, value),
        value => exports["ide_core_write"](this.command_base, 32, value));
    for(let reg = 1; reg < 8; reg++)
    {
        io.register_read(this.command_base | reg, this, read(this.command_base | reg));
        io.register_write(this.command_base | reg, this, write(this.command_base | reg));
    }
    io.register_read(this.control_base, this, read(this.control_base));
    io.register_write(this.control_base, this, write(this.control_base));
    const bus_master_base = BUS_MASTER_BASE + channel_nr * 8;
    io.register_read(bus_master_base | BMI_REG_COMMAND, this, read(bus_master_base | BMI_REG_COMMAND), undefined,
        () => exports["ide_core_read"](bus_master_base | BMI_REG_COMMAND, 32));
    io.register_write(bus_master_base | BMI_REG_COMMAND, this, write(bus_master_base | BMI_REG_COMMAND), undefined,
        value => exports["ide_core_write"](bus_master_base | BMI_REG_COMMAND, 32, value));
    io.register_read(bus_master_base | BMI_REG_STATUS, this, read(bus_master_base | BMI_REG_STATUS));
    io.register_write(bus_master_base | BMI_REG_STATUS, this, write(bus_master_base | BMI_REG_STATUS));
    io.register_read(bus_master_base | BMI_REG_PRDT, this, undefined, undefined,
        () => exports["ide_core_read"](bus_master_base | BMI_REG_PRDT, 32));
    io.register_write(bus_master_base | BMI_REG_PRDT, this, undefined, undefined,
        value => exports["ide_core_write"](bus_master_base | BMI_REG_PRDT, 32, value));

    DEBUG && Object.seal(this);
}

IDEChannel.prototype.push_irq = function()
{
    this.cpu.wm.exports["ide_push_irq"](this.channel_nr);
};

IDEChannel.prototype.get_state = function()
{
    var state = [];
    state[0] = this.master;
    state[1] = this.slave;
    state[2] = this.command_base;
    state[3] = this.irq;
    state[5] = this.control_base;
    state[7] = this.name;
    state[8] = this.device_control_reg;
    state[9] = this.prdt_addr;
    state[10] = this.dma_status;
    state[11] = this.current_interface === this.master;
    state[12] = this.dma_command;
    return state;
};

IDEChannel.prototype.set_state = function(state)
{
    this.master.set_state(state[0]);
    this.slave.set_state(state[1]);
    this.command_base = state[2];
    this.irq = state[3];
    this.control_base = state[5];
    this.name = state[7];
    this.device_control_reg = state[8];
    this.prdt_addr = state[9];
    this.dma_status = state[10];
    this.current_interface = state[11] ? this.master : this.slave;
    this.dma_command = state[12];
};

/**
 * @constructor
 * @param {IDEChannel} channel
 * @param {number} interface_nr
 * @param {boolean} is_cd
 */
function IDEInterface(channel, interface_nr, buffer, is_cd)
{
    const core = channel.cpu, core_channel = channel.channel_nr;
    define_core_fields(this, core, () => core.get_ide_addr(core_channel) + interface_nr * 44, IFACE_FIELDS);
    const exports = core.wm.exports;
    exports["ide_drive_init"](core_channel, interface_nr, !!is_cd);
    const fields = () => exports["ide_drive_fields"](core_channel, interface_nr) >>> 0;
    define_core_fields(this, core, fields, DRIVE_FIELDS);
    for(const name of Object.keys(DRIVE_DOUBLES))
    {
        const offset = DRIVE_DOUBLES[name];
        Object.defineProperty(this, name, {
            get: () => new Float64Array(core.wasm_memory.buffer, fields() + offset, 1)[0],
            set: value => { new Float64Array(core.wasm_memory.buffer, fields() + offset, 1)[0] = value; },
        });
    }
    // the data buffer: a view of the core's (a copy is set into it)
    Object.defineProperty(this, "data", {
        get: () => new Uint8Array(core.wasm_memory.buffer, exports["ide_data"](core_channel, interface_nr, -1) >>> 0,
            exports["ide_data_size"](core_channel, interface_nr)),
        set: value => {
            const at_ = exports["ide_data"](core_channel, interface_nr, value.length) >>> 0; // (first: it can grow the memory)
            new Uint8Array(core.wasm_memory.buffer, at_,
                value.length).set(value);
        },
    });

    this.channel = channel;
    this.name = channel.name + "." + interface_nr;

    /** @const @type {BusConnector} */
    this.bus = channel.bus;

    /** @const @type {number} */
    this.channel_nr = channel.channel_nr;

    /** @const @type {number} */
    this.interface_nr = interface_nr;

    /** @const @type {CPU} */
    this.cpu = channel.cpu;

    this.buffer = null;

    this.drive_connected = !!(is_cd || buffer);
    this.is_atapi = !!is_cd;
    this.status_reg = ATA_SR_DRDY|ATA_SR_DSC;
    this.is_lba = 0;
    this.sector_count_reg = 0;
    this.lba_low_reg = 0;
    this.features_reg = 0;
    this.lba_mid_reg = 0;
    this.lba_high_reg = 0;
    this.head = 0;
    this.device_reg = 0;
    this.error_reg = 0;

    /** the reads under way: id to their AbortController */
    this.in_progress_io = new Map();

    this.set_disk_buffer(buffer);

    Object.seal(this);
}

IDEInterface.prototype.has_disk = function()
{
    return !!this.buffer;
};

IDEInterface.prototype.eject = function()
{
    if(this.is_atapi && this.buffer)
    {
        this.medium_changed = MEDIUM_CHANGED_NOT_PRESENT;
        this.set_buffer(null);
        this.status_reg = ATA_SR_DRDY|ATA_SR_DSC|ATA_SR_DRQ|ATA_SR_COND;
        this.push_irq();
    }
};

IDEInterface.prototype.set_cdrom = function(buffer)
{
    if(this.is_atapi && buffer)
    {
        this.set_disk_buffer(buffer);
        this.medium_changed = MEDIUM_CHANGED_NOT_PRESENT;
    }
};

/** The image (the core knows whether there is one and its size) */
IDEInterface.prototype.set_buffer = function(buffer)
{
    this.buffer = buffer;
    this["has_disk_"] = buffer ? 1 : 0;
    this.disk_bytes = buffer ? buffer.byteLength : 0;
};

const TRANSLATION_NONE = 0;
const TRANSLATION_LBA = 1;
const TRANSLATION_LARGE = 2;

IDEInterface.prototype.get_disk_geometry = function()
{
    const mbr = this.buffer.byteLength >= 512 ? this.buffer.get_from_cache(0, 512) : undefined;

    let geometry_mbr;

    if(!mbr)
    {
        dbg_log(this.name + ": geometry: MBR not in the cache", LOG_DISK);
    }
    else if(mbr[510] === 0x55 && mbr[511] === 0xAA)
    {
        for(let i = 0; i < 4; i++)
        {
            const offset = 0x1BE + i * 16;
            const end_head = mbr[offset + 5];
            const sectors = mbr[offset + 6] & 63;
            const sector_count = (mbr[offset + 12] | mbr[offset + 13] << 8 | mbr[offset + 14] << 16 | mbr[offset + 15] << 24) >>> 0;

            if(sector_count && end_head && sectors)
            {
                const heads = end_head + 1;
                const cylinders = this.sector_count / (heads * sectors) | 0;

                if(cylinders >= 1 && cylinders <= 16383)
                {
                    geometry_mbr = { cylinders, heads, sectors };
                    break;
                }
            }
        }
    }

    let cylinders;
    let heads;
    let sectors;
    let translation;

    if(geometry_mbr && geometry_mbr.heads <= 16)
    {
        ({ cylinders, heads, sectors } = geometry_mbr);
        translation = TRANSLATION_NONE;
    }
    else
    {
        heads = 16;
        sectors = 63;
        cylinders = this.sector_count / (heads * sectors) | 0;

        if(cylinders > 16383)
        {
            cylinders = 16383;
        }
        else if(cylinders < 2)
        {
            cylinders = 2;
        }

        if(!geometry_mbr && cylinders <= 1024)
        {
            translation = TRANSLATION_NONE;
        }
        else if(geometry_mbr && cylinders * 16 <= 131072)
        {
            translation = TRANSLATION_LARGE;
        }
        else
        {
            translation = TRANSLATION_LBA;
        }
    }

    dbg_log(this.name + ": geometry " + cylinders + "/" + heads + "/" + sectors + " translation " +
        ["none", "lba", "large"][translation], LOG_DISK);

    return { cylinders, heads, sectors, translation };
};

IDEInterface.prototype.set_disk_buffer = function(buffer)
{
    if(!buffer)
    {
        return;
    }

    this.set_buffer(buffer);
    if(this.is_atapi)
    {
        this.status_reg = ATA_SR_DRDY|ATA_SR_DSC|ATA_SR_DRQ|ATA_SR_COND;
        this.error_reg = ATAPI_SK_UNIT_ATTENTION << 4;
    }
    this.sector_count = Math.ceil(this.buffer.byteLength / this.sector_size);

    if(this.is_atapi)
    {
        this.head_count = 1;
        this.sectors_per_track = 2048;
        this.cylinder_count = Math.floor(this.sector_count / this.head_count / this.sectors_per_track);
    }
    else
    {
        const { cylinders, heads, sectors, translation } = this.get_disk_geometry();

        this.cylinder_count = cylinders;
        this.head_count = heads;
        this.sectors_per_track = sectors;

        const rtc = this.cpu.devices.rtc;

        // the BIOS's translation of the drive (SeaBIOS reads it from 0x39: two bits per drive)
        const shift = (this.channel_nr * 2 + this.interface_nr) * 2;
        rtc.cmos_write(CMOS_BIOS_DISKTRANSFLAG,
                       rtc.cmos_read(CMOS_BIOS_DISKTRANSFLAG) & ~(3 << shift) |
                       translation << shift);

        if(this.channel_nr === 0)
        {
            const is_drive1 = this.interface_nr === 0;

            rtc.cmos_write(CMOS_DISK_DATA, rtc.cmos_read(CMOS_DISK_DATA) | (is_drive1 ? 0xF0 : 0x0F));
            rtc.cmos_write(is_drive1 ? CMOS_DISK_DRIVE1_TYPE : CMOS_DISK_DRIVE2_TYPE, 47);

            const drive_reg = is_drive1 ? CMOS_DISK_DRIVE1_CYL : CMOS_DISK_DRIVE2_CYL;
            rtc.cmos_write(drive_reg + 0, this.cylinder_count & 0xFF);
            rtc.cmos_write(drive_reg + 1, this.cylinder_count >> 8 & 0xFF);
            rtc.cmos_write(drive_reg + 2, this.head_count & 0xFF);
            rtc.cmos_write(drive_reg + 3, 0xFF);
            rtc.cmos_write(drive_reg + 4, 0xFF);
            rtc.cmos_write(drive_reg + 5, 0xC0 | (this.head_count > 8 ? 1 : 0) << 3);
            rtc.cmos_write(drive_reg + 6, this.cylinder_count & 0xFF);
            rtc.cmos_write(drive_reg + 7, this.cylinder_count >> 8 & 0xFF);
            rtc.cmos_write(drive_reg + 8, this.sectors_per_track & 0xFF);
        }
    }

    if(this.channel.cpu)
    {
        this.push_irq();
    }
};

IDEInterface.prototype.push_irq = function()
{
    this.channel.push_irq();
};

/** The core asks for `length` bytes of the image at `start` (read `id`) */
IDEInterface.prototype.core_read = function(id, start, length)
{
    const exports = this.cpu.wm.exports;
    if(!this.buffer)
    {
        return;
    }
    this.bus.send("ide-read-start");
    const abort = new AbortController();
    this.in_progress_io.set(id, abort);
    this.buffer.get(start, length, data =>
    {
        if(!this.in_progress_io.delete(id))
        {
            return; // (cancelled)
        }
        const at_ = exports["ide_io_buffer"](this.channel_nr, this.interface_nr, data.length) >>> 0; // (first: it can grow the memory)
        new Uint8Array(this.cpu.wasm_memory.buffer, at_,
            data.length).set(data);
        exports["ide_read_done"](this.channel_nr, this.interface_nr, id);
        this.bus.send("ide-read-end", [this.channel_nr, length, length / this.sector_size | 0]);
    }, { signal: abort.signal });
};

/** The core writes `bytes` to the image at `start`; notify: tell it when they are there */
IDEInterface.prototype.core_write = function(id, start, bytes, notify)
{
    const data = bytes.slice();
    const exports = this.cpu.wm.exports;
    if(this.buffer)
    {
        this.buffer.set(start, data, () =>
        {
            if(notify) exports["ide_write_done"](this.channel_nr, this.interface_nr, id);
        });
    }
    this.bus.send("ide-write-end", [this.channel_nr, data.length, data.length / this.sector_size | 0]);
};

IDEInterface.prototype.cancel_io_operations = function()
{
    for(const abort of this.in_progress_io.values())
    {
        abort.abort();
    }
    this.in_progress_io.clear();
    this.cpu.wm.exports["ide_cancel"](this.channel_nr, this.interface_nr);
};

IDEInterface.prototype.get_state = function()
{
    var state = [];
    state[0] = this.sector_count_reg;
    state[1] = this.cylinder_count;
    state[2] = this.lba_high_reg;
    state[3] = this.lba_mid_reg;
    state[4] = this.data_pointer;
    state[5] = 0;
    state[6] = 0;
    state[7] = 0;
    state[8] = 0;
    state[9] = this.device_reg;
    state[10] = this.error_reg;
    state[11] = this.head;
    state[12] = this.head_count;
    state[13] = !!this.is_atapi;
    state[14] = this.is_lba;
    state[15] = this.features_reg;
    state[16] = this.data.slice();
    state[17] = this.data_length;
    state[18] = this.lba_low_reg;
    state[19] = this.sector_count;
    state[20] = this.sector_size;
    state[21] = this.sectors_per_drq;
    state[22] = this.sectors_per_track;
    state[23] = this.status_reg;
    state[24] = this.write_dest;
    state[25] = this.current_command;
    state[26] = this.data_end;
    state[27] = this.current_atapi_command;
    state[28] = this.buffer;
    return state;
};

IDEInterface.prototype.set_state = function(state)
{
    this.sector_count_reg = state[0];
    this.cylinder_count = state[1];
    this.lba_high_reg = state[2];
    this.lba_mid_reg = state[3];
    this.data_pointer = state[4];
    this.device_reg = state[9];
    this.error_reg = state[10];
    this.head = state[11];
    this.head_count = state[12];
    this.is_atapi = +!!state[13];
    this.is_lba = state[14];
    this.features_reg = state[15];
    this.data = state[16];
    this.data_length = state[17];
    this.lba_low_reg = state[18];
    this.sector_count = state[19];
    this.sector_size = state[20];
    this.sectors_per_drq = state[21];
    this.sectors_per_track = state[22];
    this.status_reg = state[23];
    this.write_dest = state[24];
    this.current_command = state[25];
    this.data_end = state[26];
    this.current_atapi_command = state[27];
    this.buffer && this.buffer.set_state(state[28]);
    this.set_buffer(this.buffer);
    this.drive_connected = !!(this.is_atapi || this.buffer);
    this.medium_changed = MEDIUM_CHANGED_NONE;
    this.cancel_io_operations();
};

/** The interface of a channel and drive (the core's imports name them by number) */
IDEController.prototype.drive = function(channel_nr, drive_nr)
{
    const channel = channel_nr ? this.secondary : this.primary;
    return channel ? (drive_nr ? channel.slave : channel.master) : null;
};

/** Something the core tells about a drive (src/rust/cpu/ide.rs, ide_event): 0 the guest ejected the medium */
IDEController.prototype.core_event = function(channel_nr, drive_nr, kind)
{
    const drive = this.drive(channel_nr, drive_nr);
    if(drive && kind === 0) drive.buffer = null;
};
