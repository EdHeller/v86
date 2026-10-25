; EXPECT: pass
; NATIVE: yes
; DESC: xmm8-xmm15, SSE/SSE2 arithmetic and integer ops with REX operands, rip-relative and SIB memory operands
%include "inc/long.inc"

test_main:
    movaps xmm8, [rel fa]
    movaps xmm9, [rel fb]
    addps xmm8, xmm9                            ; 11 22 33 44
    movaps [rel out], xmm8
    CHECK_EQ32 1, dword [rel out], 0x41300000   ; 11.0
    CHECK_EQ32 2, dword [rel out + 12], 0x42300000 ; 44.0
    mulpd xmm10, xmm10                          ; xmm10 = 0 * 0
    movapd xmm10, [rel da]
    mulpd xmm10, [rel db]                       ; [2*5, 3*7] = [10, 21]
    movapd [rel out], xmm10
    mov rax, [rel out]
    mov rbx, 0x4024000000000000                 ; 10.0
    CHECK_EQ 3, rax, rbx
    mov rax, [rel out + 8]
    mov rbx, 0x4035000000000000                 ; 21.0
    CHECK_EQ 4, rax, rbx
    movdqa xmm12, [rel ia]
    paddd xmm12, [rel ib]
    movdqa [rel out], xmm12
    CHECK_EQ32 5, dword [rel out], 11
    CHECK_EQ32 6, dword [rel out + 4], 22
    pxor xmm15, xmm15
    pcmpeqd xmm15, xmm15
    psrld xmm15, 31                             ; 1 in every dword
    movdqa [rel out], xmm15
    CHECK_EQ32 7, dword [rel out + 8], 1
    pshufd xmm13, xmm12, 0x1B                   ; reverse dwords
    movdqa [rel out], xmm13
    CHECK_EQ32 8, dword [rel out], 44
    CHECK_EQ32 9, dword [rel out + 12], 11
    punpckldq xmm14, xmm13
    movdqa xmm0, xmm14
    movdqa [rel out], xmm0
    CHECK_EQ32 10, dword [rel out + 4], 44
    lea rsi, [rel ia]
    mov ecx, 1
    movdqu xmm11, [rsi + rcx * 4]               ; sib addressing, unaligned
    movdqu [rel out], xmm11
    CHECK_EQ32 11, dword [rel out], 2
    movd xmm9, r12d
    mov r12, 0xFFFFFFFF12345678
    movd xmm9, r12d
    movd r13d, xmm9
    CHECK_EQ 12, r13, 0x12345678
    movq xmm9, r12
    movq r14, xmm9
    CHECK_EQ 13, r14, 0xFFFFFFFF12345678
    cvtsi2sd xmm10, r12                         ; REX.W + REX.R
    cvttsd2si r15, xmm10
    CHECK_EQ 14, r15, 0xFFFFFFFF12345678
    movmskps eax, xmm8
    CHECK_EQ 15, rax, 0
    movaps xmm0, xmm8
    xorps xmm8, xmm8
    movaps xmm8, xmm0
    movq r10, xmm8                              ; low qword = 11.0f, 22.0f
    mov rbx, 0x41B0000041300000
    CHECK_EQ 16, r10, rbx
    PASS
section .data
align 16
fa:  dd 1.0, 2.0, 3.0, 4.0
fb:  dd 10.0, 20.0, 30.0, 40.0
da:  dq 2.0, 3.0
db:  dq 5.0, 7.0
ia:  dd 1, 2, 3, 4
ib:  dd 10, 20, 30, 40
out: times 4 dd 0
