; EXPECT: xfail
; DESC: SSE3 ADDSUBPS/PD, HADDPS/PD, HSUBPS/PD (0F D0, 0F 7C, 0F 7D stubs), LDDQU (F2 0F F0), MOVDDUP/MOVSHDUP/MOVSLDUP
%include "inc/real.inc"

test_main:
    SSE_ON
    movaps xmm0, [fa]
    addsubps xmm0, [fb]
    CHECK_XMM 1, xmm0, 0xC1100000, 0x41B00000, 0xC1D80000, 0x42300000
    movapd xmm0, [da]
    addsubpd xmm0, [db]
    CHECK_XMM 2, xmm0, 0, 0xC0220000, 0, 0x40360000
    movaps xmm0, [fa]
    haddps xmm0, [fb]
    CHECK_XMM 3, xmm0, 0x40400000, 0x40E00000, 0x41F00000, 0x428C0000
    movaps xmm0, [fa]
    hsubps xmm0, [fb]
    CHECK_XMM 4, xmm0, 0xBF800000, 0xBF800000, 0xC1200000, 0xC1200000
    movapd xmm0, [da]
    haddpd xmm0, [db]
    CHECK_XMM 5, xmm0, 0, 0x40080000, 0, 0x403E0000
    movapd xmm0, [da]
    hsubpd xmm0, [db]
    CHECK_XMM 6, xmm0, 0, 0xBFF00000, 0, 0xC0240000
    lddqu xmm0, [bytes + 1]                     ; unaligned 16-byte load
    CHECK_XMM 7, xmm0, 0x04030201, 0x08070605, 0x0C0B0A09, 0x100F0E0D
    movddup xmm0, [da]
    CHECK_XMM 8, xmm0, 0, 0x3FF00000, 0, 0x3FF00000
    movshdup xmm0, [fa]
    CHECK_XMM 9, xmm0, 0x40000000, 0x40000000, 0x40800000, 0x40800000
    movsldup xmm0, [fa]
    CHECK_XMM 10, xmm0, 0x3F800000, 0x3F800000, 0x40400000, 0x40400000
    PASS
align 16
fa: dd 1.0, 2.0, 3.0, 4.0
fb: dd 10.0, 20.0, 30.0, 40.0
da: dq 1.0, 2.0
db: dq 10.0, 20.0
bytes: db 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17
