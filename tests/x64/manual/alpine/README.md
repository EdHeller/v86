# Booting Alpine Linux x86_64

`../alpine.js` boots an Alpine "Virtual" ISO (https://dl-cdn.alpinelinux.org/alpine/latest-stable/releases/x86_64/)
through SeaBIOS (isolinux) or, with `ALP_KERNEL` / `ALP_INITRD` / `ALP_CMDLINE`, directly from the kernel and
initramfs inside the ISO (`/boot/vmlinuz-virt`, `/boot/initramfs-virt`). Not part of the automated run.

    node tests/x64/manual/alpine.js alpine-virt-3.24.3-x86_64.iso 600

Useful settings: `ALP_SERIAL='[[sec,"text"],..]'` types on the serial console (isolinux prompt: send
`virt console=ttyS0,115200` within a second, the label times out), `ALP_SAVE=file:sec` / `ALP_LOAD=file` for a
logged in snapshot, `ALP_TRACE=1` (last 256 instructions after a crash), `ALP_TRACE_SYSCALL=<nr>` and
`ALP_WATCH=<va>` (debug wasm only), `ALP_DUMP=<va>:<bytes>` (guest memory read through cr3),
`ALP_SYSRQ='[[sec,"t"]]'` (Alt+SysRq on the keyboard: task dump), `ALP_UNSUPPORTED=1`, `JIT64=<threshold>`.

Status (6.18.58 virt kernel, Alpine 3.24.3): boots to the `localhost login:` prompt in about 7 minutes,
root login works. Things that had to be fixed on the way:

- compatibility mode: the 16-bit instruction pointer wrap of the interpreter loop also hit 64-bit code
  (the decompressor's far return into `startup_64` landed 64k too low);
- `xadd` modified its register operand before the memory write that can page fault, so a restarted
  `lock xadd` (first write to a copy-on-write page after fork) added garbage to musl's lock words and
  deadlocked `nlplug-findfs` (regression test `long/31_restart_faults`, fuzz family `atomic`).

Shell loop benchmark (`i=$((i+1))` x 40000 in busybox ash): 54 s with `JIT64=4294967295` (jit64 off), 27 s with it on.
