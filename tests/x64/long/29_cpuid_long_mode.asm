; EXPECT: xfail
; ISSUE: copy/v86#133
; DESC: CPUID extended leaves advertise long mode, SYSCALL/SYSRET, NX and the 48-bit address sizes
bits 16
org 0x7C00
%include "inc/harness.inc"

    cli
    mov eax, 0x80000000
    cpuid
    cmp eax, 0x80000008
    jae .have_leaves
    FAIL16 1                                    ; max extended leaf must cover 0x80000008
.have_leaves:
    mov eax, 0x80000001
    cpuid
    test edx, 1 << 29                           ; LM
    jnz .lm
    FAIL16 2
.lm:
    test edx, 1 << 11                           ; SYSCALL/SYSRET
    jnz .sc
    FAIL16 3
.sc:
    test edx, 1 << 20                           ; NX
    jnz .nx
    FAIL16 4
.nx:
    mov eax, 0x80000008
    cpuid
    cmp ah, 48                                  ; AH = linear address bits (48, or 57 with LA57)
    jae .virt
    FAIL16 5
.virt:
    PASS_REAL
