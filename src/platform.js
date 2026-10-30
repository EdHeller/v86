// The platform description the ACPI tables of the UEFI firmware are built
// from (src/acpi_tables.js, from v64): v86's PC (i440FX) as OVMF finds it.

/** @const */ export const LAPIC_ADDRESS = 0xFEE00000;
/** @const */ export const IOAPIC_ADDRESS = 0xFEC00000;
/** @const */ export const IOAPIC_ID = 0;
/** @const */ export const PCI_MMIO_END = 0xFEBFFFFF;
/** @const */ export const ACPI_SCI_IRQ = 9;
/** @const */ export const ACPI_SMI_CMD_PORT = 0xB2;
/** @const */ export const FW_CFG_PORT = 0x510;
/** @const */ export const RESET_PORT = 0xCF9;
/** @const */ export const RESET_VALUE = 0x06;
/** @const */ export const PCI_LINK_IRQS = [10, 11];
/** @const */ export const ICH9_RCBA_ADDRESS = 0xFED1C000;
/** @const */ export const ICH9_RCBA_SIZE = 0x4000;
/** @const */ export const TPM_CRB_BASE = 0xFED40000;
/** @const */ export const TPM_CRB_SIZE = 0x5000;
/** @const */ export const CMOS_CENTURY = 0x32;
/** @const */ export const HPET_ADDRESS = 0xFED00000;
/** @const */ export const HPET_SIZE = 0x400;
/** @const */ export const HPET_BLOCK_ID = 0x8086A201;

/**
 * @typedef {{
 *     machine: string,
 *     qemu_compatible: boolean,
 *     apic_ids: !Array<number>,
 *     uarts: !Array<{index: number, port: number, irq: number}>,
 *     parallel_ports: !Array<{index: number, port: number, irq: number}>,
 *     tpm: boolean,
 *     ecam: ?{base: number, size: number},
 *     pm: {length: number, gpe0_offset: number, gpe0_base: number, gpe0_length: number, acpi_enable: number, acpi_disable: number},
 *     sleep_states: !Array<{state: number, slp_typ: number, supported: boolean}>,
 *     hpet: boolean,
 *     pci_mmio_start: number,
 * }}
 */
export var Platform;

/**
 * @param {{cores: number, memory_size: number, tpm: boolean}} options
 * @return {Platform}
 */
export function create_platform(options)
{
    return {
        machine: "i440fx",
        qemu_compatible: true,
        apic_ids: Array.from({ length: options.cores }, (_, i) => i),
        uarts: [{ index: 0, port: 0x3F8, irq: 4 }],
        parallel_ports: [],
        tpm: options.tpm,
        ecam: null,
        // the GPE block of v86's ACPI (src/acpi.js) is at a fixed port
        pm: { length: 0x40, gpe0_offset: -1, gpe0_base: 0xAFE0, gpe0_length: 4, acpi_enable: 0xF1, acpi_disable: 0xF0 },
        sleep_states: [{ state: 5, slp_typ: 0, supported: true }],
        hpet: false,
        // OVMF's 32-bit PCI window: from 2 GiB, or above RAM
        pci_mmio_start: Math.max(0x80000000, Math.ceil(options.memory_size / 0x10000000) * 0x10000000),
    };
}

/**
 * @param {Platform} platform
 * @param {number} pm_base
 * @return {{base: number, length: number}}
 */
export function gpe0_block(platform, pm_base)
{
    const pm = platform.pm;
    return { base: pm.gpe0_offset >= 0 ? pm_base + pm.gpe0_offset : pm.gpe0_base, length: pm.gpe0_length };
}

/**
 * @param {Platform} platform
 * @return {!Array<!Array<number>>}
 */
export function pci_memory_windows(platform)
{
    return [[platform.pci_mmio_start, PCI_MMIO_END]];
}
