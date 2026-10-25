; EXPECT: xfail
; QEMU: xfail  (qemu only enforces alignment for the plain move instructions)
; ISSUE: copy/v86#1356
; DESC: legacy SSE arithmetic/shuffle instructions with misaligned 16-byte memory operands raise #GP
%include "inc/real.inc"

test_main:
    SSE_ON
    TRAP_BEGIN
        cvtdq2ps xmm0, [buf + 4]
    TRAP_END 1, 13
    TRAP_BEGIN
        punpcklbw xmm0, [buf + 1]
    TRAP_END 2, 13
    TRAP_BEGIN
        pshufd xmm0, [buf + 4], 0
    TRAP_END 3, 13
    TRAP_BEGIN
        paddd xmm0, [buf + 12]
    TRAP_END 4, 13
    PASS
align 16
buf: times 48 db 0
