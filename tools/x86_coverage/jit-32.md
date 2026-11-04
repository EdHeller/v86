# 32-bit: the interpreter against the JIT

`run.mjs 32 jit`: 300 passes over the probes interpreted (disable_jit) and 300 compiled (the pages
compiled after little heat); the last passes compared. Interpreted 24.3 s, compiled 30.8 s.
Left out: time stamps, random numbers, port input, saved FPU environments (their instruction pointers differ).
Dump offsets: eax..edi at 0..31, eflags 32, xmm0..7 at 48, memory at 176.

**0 forms differ** (interpreted / compiled)

| Form | Instruction | Differences |
|---|---|---|

