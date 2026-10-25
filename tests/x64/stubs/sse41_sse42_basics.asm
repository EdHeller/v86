; EXPECT: pass
; DESC: SSE4.1/4.2 PMULLD, PMAXSD/PMINSD, PTEST, PBLENDW, ROUNDPS, PEXTRD/PINSRD, PMOVZX/SX, PCMPEQQ, CRC32 (0F 38 / 0F 3A stubs)
%include "inc/real.inc"

test_main:
    SSE_ON
    movdqa xmm0, [md_a]
    pmulld xmm0, [md_b]
    CHECK_XMM 1, xmm0, 21, 0xFFFFFFE8, 0, 0xFFFFFFE7
    movdqa xmm0, [mx_a]
    pmaxsd xmm0, [mx_b]
    CHECK_XMM 2, xmm0, 1, 2, 3, 4
    movdqa xmm0, [mx_a]
    pminsd xmm0, [mx_b]
    CHECK_XMM 3, xmm0, 0xFFFFFFFF, 0xFFFFFFFE, 0xFFFFFFFD, 0xFFFFFFFC
    pcmpeqd xmm0, xmm0                          ; all ones
    pxor xmm1, xmm1
    ptest xmm0, xmm1
    jnz .bad_zf1
    jc .ptest1
.bad_zf1:
    FAIL16 5                                    ; expected ZF=1 CF=1
.ptest1:
    movdqa xmm0, [one]
    ptest xmm1, xmm0                            ; xmm1 = 0, xmm0 = 1: ZF=1, CF=0
    jnz .bad_zf2
    jnc .ptest2
.bad_zf2:
    FAIL16 6
.ptest2:
    ptest xmm0, xmm0                            ; 1 & 1 != 0: ZF=0, CF=1
    jz .bad_zf3
    jc .ptest3
.bad_zf3:
    FAIL16 7
.ptest3:
    movdqa xmm0, [blend_a]
    pblendw xmm0, [blend_b], 0xAA
    CHECK_XMM 8, xmm0, 0xBBBBAAAA, 0xBBBBAAAA, 0xBBBBAAAA, 0xBBBBAAAA
    roundps xmm0, [rf], 1                       ; floor
    CHECK_XMM 9, xmm0, 0x3F800000, 0xC0000000, 0x40000000, 0xC0400000
    movdqa xmm1, [md_a]
    pextrd eax, xmm1, 2
    CHECK16 10, eax, 0x10000
    mov eax, 0x55AA55AA
    pinsrd xmm1, eax, 1
    CHECK_XMM 11, xmm1, 3, 0x55AA55AA, 0x10000, 5
    pmovzxbw xmm0, [pm]
    CHECK_XMM 12, xmm0, 0x000100FF, 0x00030002, 0x00050004, 0x00070006
    pmovsxbw xmm0, [pm]
    CHECK_XMM 13, xmm0, 0x0001FFFF, 0x00030002, 0x00050004, 0x00070006
    movdqa xmm0, [q_a]
    pcmpeqq xmm0, [q_b]
    CHECK_XMM 14, xmm0, 0xFFFFFFFF, 0xFFFFFFFF, 0, 0
    xor eax, eax
    mov cl, 1
    crc32 eax, cl
    CHECK16 15, eax, 0xF26B8303
    PASS
align 16
md_a: dd 3, -4, 0x10000, 5
md_b: dd 7, 6, 0x10000, -5
mx_a: dd 1, -2, 3, -4
mx_b: dd -1, 2, -3, 4
one: dd 1, 0, 0, 0
blend_a: times 8 dw 0xAAAA
blend_b: times 8 dw 0xBBBB
rf: dd 1.5, -1.5, 2.5, -2.5
pm: db 0xFF,1,2,3,4,5,6,7,0,0,0,0,0,0,0,0
q_a: dq 5, 7
q_b: dq 5, 8
