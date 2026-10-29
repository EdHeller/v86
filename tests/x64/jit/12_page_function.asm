; EXPECT: pass
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: a real kernel function (lea forms with 32 bit results, branches into the middle of a page) run on random strings
%include "inc/long.inc"
%define EXPECTED 0x07000200c0040001

test_main:
    xor r14d, r14d
    mov r15d, 1280
.loop:
    mov eax, r15d
    and eax, 63
    imul eax, 48
    lea rdi, [rel strs]
    add rdi, rax
    mov eax, r15d
    imul eax, 7
    shr eax, 2
    and eax, 63
    imul eax, 48
    lea rsi, [rel strs]
    add rsi, rax
    call fn
    rol r14, 5
    xor r14, rax
    dec r15d
    jnz .loop
    CHECK_EQ 1, r14, EXPECTED
    PASS

fn:
    incbin "jit/data/kernel_scan_fn.bin"

section .data
align 16
strs:
    incbin "jit/data/scan_strs.bin"
