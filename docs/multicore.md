# Cores that take turns (`cpu_cores: N`)

`new V86({ acpi: true, cpu_cores: 4, ... })` gives the guest N processors (1-8). The cores run one after the other on the
emulator's thread, 20,011 instructions at a time (`src/rust/cpu/smp.rs`). This does not make anything faster: it exists
so that guests which want several processors (a Windows or Linux SMP kernel, a multi-core test) work, and as the first
step towards cores in Web Workers.

Needs `acpi: true` (the cores are found through the ACPI tables and the APIC). Saved states with more than one core are
not supported yet (only the first core's state is saved).

## How it works

- **State.** The registers and the rest of a core's state live in the fixed block of the module's memory
  (`global_pointers.rs`, bytes 64..1792). A switch copies the running core's block out and the next one in. The fields that belong to
  the machine stay (`SHARED` in `smp.rs`: memory size, acpi/apic enable, the instruction counter behind the TSC, the SVGA dirty
  range, `current_tsc`). The translation caches (both TLBs, the instruction fetch cache) are flushed on a switch, and so are the
  slot caches of the 64-bit JIT (`jit64::core_switched`); compiled code stays valid for every core.
- **APIC.** One local APIC per core (`apic.rs`): ids 0..N-1, physical and logical (flat and cluster) destinations, the
  shorthands (self, all, all but self), lowest priority, INIT and start-up IPIs, NMI/SMI flags, a timer per core. The I/O APIC routes
  to the cores its redirection entries name. The legacy PIC is wired to core 0 only.
- **Start-up.** After a reset only core 0 runs, the others wait for INIT + start-up IPI. SeaBIOS starts them (the core count is in
  CMOS 0x5F) and Linux does it again.
- **Scheduler.** `smp::main_loop`: each round runs the devices' timers once, then each runnable core for a slice. A core is
  runnable when it is not halted, or halted with interrupts enabled and one pending (or an NMI). When no core is runnable the loop
  returns the time until the next device timer.
- **CPUID.** Leaf 1: initial APIC id of the running core, logical processor count, HTT bit; leaf 4: cores in the package.

## Pitfall found on the way

The APIC register writes used to call `handle_irqs()` from the memory-mapped write itself. In compiled code that delivers the interrupt in the
middle of an instruction: the compiled function then goes on with an `eip` whose page is the handler's, and runs the wrong code (found with
`tests/x64/smp/03_broadcast_ipis.asm`, and in Linux as a crash in `native_send_call_func_ipi` once all cores were up and the kernel
used shorthand IPIs). The write now only sets `IRQ_CHECK`; the loops that call `cycle_internal` act on it between instructions. Lowering the
instruction limit of compiled functions (`JIT_LOOP_LIMIT`, the compiled code reads it at run time) to get shorter slices stopped SeaBIOS from
booting with 2 cores (cause not found), so slices end at `SLICE` instructions or at the first compiled-function exit.

## Limits

- No parallelism, and a spinning core burns its slice (PAUSE does not yield yet).
- A core that gets INIT loses all state (the x87/SSE state would survive on hardware).
- NMI and SMI to other cores are recorded but not delivered.
- x2APIC is not implemented.

## Tests

`tests/x64/smp/01_two_cores.asm`: CPUID and APIC ids, INIT + start-up IPI, fixed IPI to a physical destination, broadcast, and an IPI to
a core that does not exist. `smp/02` (registers, xmm and x87 state of three cores survive the switches) and `smp/03` (the same while the cores send each other broadcast IPIs). Linux 32-bit with 2, 3 and 4 cores boots and runs busy loops on all of them; `?cores=N` on `examples/alpine64.html`, then `nproc`.
