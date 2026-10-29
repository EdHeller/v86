# Cores in Web Workers (`cpu_workers`)

`new V86({ acpi: true, cpu_cores: 4, cpu_workers: true, wasm_path: "v86-parallel.wasm", vcpu_worker_url: "vcpu-worker.js", ... })` runs the
first core and every device on the page's thread (as before) and each other core in a Web Worker, in parallel. Experimental.

Needs: the page is cross-origin isolated (`Cross-Origin-Opener-Policy: same-origin`, `Cross-Origin-Embedder-Policy: require-corp`;
`examples/serve.py` sends them), `SharedArrayBuffer`, `acpi: true`, and the two files of the parallel build:

    make build/v86-parallel.wasm build/vcpu-worker.js build/libv86.js

If the parallel build is not the one loaded, `cpu_workers` is ignored and the cores take turns on one thread ([multicore.md](multicore.md)).

## What runs where

- **Machine instance** (page thread, `v86-parallel.wasm` at base 0): core 0, every device, the PIC, the I/O APIC and the local APICs of all
  cores (`apic.rs` keeps them in one array), the clock, the JIT (32-bit and 64-bit) of core 0.
- **vCPU worker k** (`src/parallel/vcpu_worker.js`): core k in its own instance of the same module, in the same shared memory. Interpreter
  only for now (no JIT in the workers). It shares the guest RAM with the machine and nothing else.

`tools/parallel_wasm.mjs` turns the Rust build (linked with `--import-memory --emit-relocs`, `+atomics`) into `v86-parallel.wasm`: the memory
becomes shared and every place that holds the address of a static is recorded. `src/parallel/relocate.js` instantiates the module again
for each worker at a new base in the shared memory, so each instance has its own statics, stack, heap and CPU state block (the block of
core k is in the low slot k: `global_pointers.rs` defines the state block as a static in this build, `STATE_BLOCK`). The normal
`v86.wasm` is unchanged.

## What a worker asks the machine thread for

A worker has no devices. These go through a request slot of the control block (`src/parallel/control.js`, a `SharedArrayBuffer`), answered
by `ParallelMachine.service()` (`src/parallel/machine.js`) between the machine thread's slices:

- I/O ports and memory mapped device accesses;
- the registers of its local APIC (the APIC of core k lives in the machine instance: `machine_apic_read32/write32` run with the current
  core set to k, so IPIs, EOI, the timer and the I/O APIC routing work as in the cooperative mode), the I/O APIC registers;
- acknowledging the highest pending interrupt (`machine_apic_ack`), and INIT / start-up IPIs (`machine_take_events`).

The machine thread tells a worker that it has something for it with the `PENDING + core` word of the control block (`machine_pending_mask`).
A halted worker sleeps in `Atomics.wait` on it.

## Memory model

- LOCKed instructions and `xchg` with memory are atomic: the 32-bit interpreter commits through compare-exchange (`memory::atomic_rmw`,
  `atomic_cas`; the closures of xchg/xadd/cmpxchg start from the registers of the first attempt), long mode snapshots the registers,
  remembers the first read of the instruction and commits the write with a compare-exchange; on a conflict the instruction runs again
  (`exec_locked` in `long_mode.rs`). `bts/btr/btc` on memory are atomic too. Compiled code does not contain LOCKed instructions or `xchg` with
  memory: the 32-bit JIT ends its blocks before them (`needs_atomic_interpretation`), jit64's `decode` refuses them.
- The accessed and dirty bits of page table entries are set with an atomic OR (`memory::set_entry_bits`).
- Plain accesses are plain: this is x86's TSO on an x86 host. **On a host with a weaker memory model (arm64) cores can see each other's
  writes out of order: not handled yet.**
- Unaligned locked operations (crossing a 32 bit boundary) are not atomic.

## Not done

- JIT in the workers, and coherence of compiled code between instances (a write by one core to a page another core compiled).
- Saved states with workers, a reset while workers run, `cpu_cores` > 8.
- Ordering fences for weakly ordered hosts.
- Only about 1 ms per request of latency: a worker waits for the machine thread for every port or APIC access.

## Tests

`tests/x64/smp/10_worker_core.asm` (INIT/SIPI, IPIs), `11_lock_counter.asm` and `12_lock_counter_long.asm` (atomics between a worker core and the
boot core in 32-bit and long mode), `tests/x64/js/parallel_instances.js` (two instances, one memory, separate statics and state).
Linux: `?arch=x86&cores=2&workers=1` on `examples/alpine64.html` served by `examples/serve.py`.
