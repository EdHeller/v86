; EXPECT: pass
; NATIVE: yes
; DESC: x87 FPU in 64-bit mode: rip-relative operands, fild/fistp 64-bit integers, comparisons, fxsave/fxrstor of xmm8-15
%include "inc/long.inc"

test_main:
    fld qword [rel d1]
    fadd qword [rel d2]                         ; 1.5 + 2.25 = 3.75
    fstp qword [rel out]
    mov rax, [rel out]
    mov rbx, 0x400E000000000000                 ; 3.75
    CHECK_EQ 1, rax, rbx
    fild qword [rel big]                        ; 2^40 + 1
    fld1
    faddp st1, st0
    fistp qword [rel out]
    CHECK_EQ 2, qword [rel out], 0x10000000002
    fld dword [rel f9]
    fsqrt
    fstp dword [rel out]
    CHECK_EQ32 3, dword [rel out], 0x40400000   ; 3.0
    fld qword [rel d1]
    fld qword [rel d2]
    fxch st1
    fcomip st0, st1                             ; compare 1.5 with 2.25: below
    fstp st0
    CHECK_CC 4, b
    CHECK_CC 5, nz
    fldz
    fld1
    fdivp st1, st0                              ; 0 / 1
    fnstsw ax
    fstp qword [rel out]
    CHECK_EQ 6, qword [rel out], 0
    fnstcw word [rel out]
    movzx eax, word [rel out]
    CHECK_EQ 7, rax, 0x37F
    ; fxsave / fxrstor keep xmm8-xmm15 in 64-bit mode
    movdqa xmm8, [rel pattern]
    movdqa xmm15, [rel pattern + 16]
    fxsave [rel area]
    pxor xmm8, xmm8
    pxor xmm15, xmm15
    fxrstor [rel area]
    movdqa [rel out], xmm8
    movdqa [rel out + 16], xmm15
    CHECK_EQ 8, qword [rel out], 0x1111111111111111
    CHECK_EQ 9, qword [rel out + 16], 0x3333333333333333
    PASS
section .data
align 16
d1: dq 1.5
d2: dq 2.25
big: dq 0x10000000001
f9: dd 9.0
align 16
pattern: dq 0x1111111111111111, 0x2222222222222222, 0x3333333333333333, 0x4444444444444444
align 16
out: times 8 dq 0
align 16
area: times 64 dq 0
