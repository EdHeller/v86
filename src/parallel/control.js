// The control block shared by the machine thread and the vCPU workers (docs/parallel.md): one Int32Array over a SharedArrayBuffer.
//
// words 0..7:   PENDING + core: the machine has something for the core (an interrupt, INIT, a start-up IPI); written by the machine
// word 8:       STOP: 1 asks the workers to end
// word 9:       WAKE: bumped by a worker that posted a request (the machine may wait on it)
// word 16 +:    one request slot per core (SLOT_WORDS words): state, kind, a0..a4, r0, r1
//               state 0 free, 1 request posted by the worker, 2 answered by the machine

export const MAX_CORES = 8;
export const PENDING = 0;
export const STOP = 8;
export const WAKE = 9;
export const SLOT_BASE = 16;
export const SLOT_WORDS = 16;
export const CONTROL_WORDS = SLOT_BASE + MAX_CORES * SLOT_WORDS;
export const CONTROL_BYTES = CONTROL_WORDS * 4;

export const SLOT_STATE = 0;
export const SLOT_KIND = 1;
export const SLOT_ARG = 2; // a0..a4
export const SLOT_RESULT = 7; // r0, r1

export const REQ_PORT_READ8 = 1, REQ_PORT_READ16 = 2, REQ_PORT_READ32 = 3;
export const REQ_PORT_WRITE8 = 4, REQ_PORT_WRITE16 = 5, REQ_PORT_WRITE32 = 6;
export const REQ_MMAP_READ8 = 7, REQ_MMAP_READ32 = 8;
export const REQ_MMAP_WRITE8 = 9, REQ_MMAP_WRITE16 = 10, REQ_MMAP_WRITE32 = 11, REQ_MMAP_WRITE64 = 12, REQ_MMAP_WRITE128 = 13;
export const REQ_APIC_READ = 14, REQ_APIC_WRITE = 15, REQ_IOAPIC_READ = 16, REQ_IOAPIC_WRITE = 17;
export const REQ_APIC_ACK = 18, REQ_TAKE_EVENTS = 19, REQ_FAILED = 20;
