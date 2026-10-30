# UEFI

`firmware: { code, vars }` replaces SeaBIOS with OVMF (edk2's X64 virtual
machine firmware, as QEMU 9.2.0 ships it, `bios/ovmf-code.fd` and
`bios/ovmf-vars.fd`; licenses in `bios/OVMF-LICENSES.txt`). Without the
option the machine boots SeaBIOS exactly as before.

```js
new V86({
    firmware: { code: { url: "bios/ovmf-code.fd" }, vars: { url: "bios/ovmf-vars.fd" } },
    acpi: true,
    memory_size: 512 << 20,
    cdrom: { url: "alpine-virt-x86_64.iso" },        // boots its EFI loader
    // or a kernel, handed over like QEMU's -kernel (fw_cfg):
    // bzimage: { url: ... }, initrd: { url: ... }, cmdline: "console=ttyS0",
});
```

How it works (ported from v64, `docs/uefi.md` there):

- **Code in device memory.** The flash is device memory
  (`src/rust/cpu/mmio_ram.rs`) ending at 4 GiB; the variable store sits below
  the code and is writable memory (OVMF keeps its variables for the session).
  Instruction fetches from a region go to its memory (`memory::code_phys`):
  the interpreter reads it, the 64-bit page tier compiles from it, and data
  accesses still go through the memory map. The last 128 KiB of the code also
  appear at 0xE0000.
- **fw_cfg.** Revision 1 (no DMA), `etc/e820`, the ACPI tables through QEMU's
  table loader (`src/acpi_tables.js`, `src/platform.js`: FADT with the PM
  block at 0x600, DSDT with the PCI links, MADT), and the -kernel items.
- **Chipset.** OVMF expects QEMU's PC: the PIIX4 power management function
  at 00:01.3 with a PMBA it programs (the PM block moves there), the primary
  IDE channel present without drives, PIIX3's IDE decode bits set, the
  UART's loopback status, the Bochs VGA's data alias at port 0x1D0.
- **Debug log.** Port 0x402 reads 0xE9 and OVMF's log arrives as
  `debug-console-output-byte` events.

Test: `node tests/uefi.mjs` (Alpine x86_64 kernel, initramfs and ISO) boots to
the login prompt with ACPI and the CD.

# Q35

`machine_type: "q35"` (with `acpi: true`) gives the machine QEMU's Q35
chipset (`src/q35.js`, after v64's): the MCH at 00:00.0 with the BIOS shadow
registers and the PCI Express configuration window (PCIEXBAR; SeaBIOS puts
it at 0xB0000000, OVMF at 0xE0000000, decoded before any BAR), and the ICH9
LPC bridge at 00:1f.0 with PMBASE/ACPI_CNTL (the PM block at 0x600), the
PIRQA-H route registers and the root complex register block. PCI interrupts
follow QEMU's wiring: devices 0-24 rotate their pins over PIRQE-H, devices
25-31 use the RCBA route registers; PIRQn is IOAPIC input 16 + n and the ISA
IRQ of its route register at the PIC. The ACPI tables (Q35 DSDT with PIC and
APIC routing, MCFG) come through the table loader with both firmwares.
Disks and the CD stay on v86's PIIX IDE function (no AHCI). With UEFI the
display is the `bochs_vga` plugin, whose BAR OVMF moves out of the ECAM
window. Test: `node tests/q35.mjs` (SeaBIOS), `UEFI=1 node tests/q35.mjs`.

# TPM 2.0

`tpm: { wasm: { url: "bios/libtpms.wasm" } }` (with `acpi: true`, on Q35 or
with UEFI, where the ACPI tables come through the table loader) adds v64's
TPM 2.0: the CRB interface at 0xFED40000 in front of libtpms compiled to
WebAssembly (`src/tpm/`, `bios/libtpms.wasm`, licenses in
`bios/LIBTPMS-LICENSES.txt`), a TPM2 ACPI table with its event log area,
and the `MSFT0101` device. Its permanent state lives in memory for the
session. OVMF finds it ("TPM2 detected", TPM2_Startup, PCR banks, measured
boot); Linux needs `tpm_crb`, which Alpine's virt kernel does not ship.
Test: `UEFI=1 node tests/tpm.mjs`.
