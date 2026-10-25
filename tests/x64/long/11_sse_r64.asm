; EXPECT: pass
; NATIVE: yes
; DESC: SSE2 with 64-bit GPRs: movq xmm<->r64, cvtsi2sd/cvttsd2si with REX.W, pextrq-free paths
%include "inc/long.inc"

test_main:
    mov rax, 0x0123456789ABCDEF
    movq xmm0, rax
    movq rbx, xmm0
    CHECK_EQ 1, rbx, 0x0123456789ABCDEF
    mov rax, -5
    cvtsi2sd xmm1, rax
    cvttsd2si rcx, xmm1
    CHECK_EQ 2, rcx, 0xFFFFFFFFFFFFFFFB
    mov rax, 0x100000000
    cvtsi2sd xmm1, rax                          ; needs the 64-bit form to be exact
    cvttsd2si rcx, xmm1
    CHECK_EQ 3, rcx, 0x100000000
    mov rax, 0x100000001
    cvtsi2ss xmm2, rax
    cvttss2si rcx, xmm2
    CHECK_EQ 4, rcx, 0x100000000                ; float rounds to 2^32
    movd xmm3, eax
    movd ecx, xmm3
    CHECK_EQ 5, rcx, 1
    movq xmm8, rax                              ; xmm8-xmm15 need REX.R/B
    movq xmm9, xmm8
    movq r14, xmm9
    CHECK_EQ 6, r14, 0x100000001
    pcmpeqd xmm15, xmm15
    movq r13, xmm15
    CHECK_EQ 7, r13, 0xFFFFFFFFFFFFFFFF
    psrldq xmm15, 8
    movq r13, xmm15
    CHECK_EQ 8, r13, 0xFFFFFFFFFFFFFFFF         ; psrldq moves the high qword down
    movdqa xmm10, xmm15
    pxor xmm10, xmm10
    movq r13, xmm10
    CHECK_EQ 9, r13, 0
    PASS
