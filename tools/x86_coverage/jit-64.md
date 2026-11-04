# 64-bit: the interpreter against the JIT

`run.mjs 64 jit`: 6 passes over the probes interpreted (disable_jit) and 6 compiled (the pages
compiled after little heat); the last passes compared. Interpreted 4.0 s, compiled 3.6 s.
Left out: time stamps, random numbers, port input, saved FPU environments (their instruction pointers differ).
Dump offsets: rax..r15 at 0..127, rflags 128, xmm0..15 at 144, memory at 400.

**0 forms differ** (interpreted / compiled)

| Form | Instruction | Differences |
|---|---|---|

