; EXPECT: xfail
; DESC: SSSE3 PHADDW/D, PHSUBW/D, PHADDSW, PMADDUBSW (0F 38 01..07, 04)
; ISSUE: copy/v86#1038
%include "inc/real.inc"

test_main:
    SSE_ON
    movdqa xmm0, [w_a]
    phaddw xmm0, [w_b]
    CHECK_XMM 1, xmm0, 0x00070003, 0x000F000B, 0x0046001E, 0x0096006E
    movdqa xmm0, [d_a]
    phaddd xmm0, [d_b]
    CHECK_XMM 2, xmm0, 3, 7, 30, 70
    movdqa xmm0, [w_c]
    phsubw xmm0, [w_d]
    CHECK_XMM 3, xmm0, 0x000F0007, 0x001F0017, 0x00C60063, 0x018C0129
    movdqa xmm0, [d_c]
    phsubd xmm0, [d_d]
    CHECK_XMM 4, xmm0, 7, 15, 99, 198
    movdqa xmm0, [sat_a]
    pxor xmm1, xmm1
    phaddsw xmm0, xmm1                          ; saturating
    CHECK_XMM 5, xmm0, 0x80007FFF, 0, 0, 0
    movdqa xmm0, [ub]                           ; unsigned bytes
    movdqa xmm1, [sb]                           ; signed bytes
    pmaddubsw xmm0, xmm1
    CHECK_XMM 6, xmm0, 0xFFF97FFF, 0, 0, 0
    PASS
align 16
w_a: dw 1,2,3,4,5,6,7,8
w_b: dw 10,20,30,40,50,60,70,80
d_a: dd 1,2,3,4
d_b: dd 10,20,30,40
w_c: dw 10,3,20,5,30,7,40,9
w_d: dw 100,1,200,2,300,3,400,4
d_c: dd 10,3,20,5
d_d: dd 100,1,200,2
sat_a: dw 0x7FFF,1,0x8000,0xFFFF,0,0,0,0
ub: db 255,255,2,3,0,0,0,0,0,0,0,0,0,0,0,0
sb: db 127,127,4,-5,0,0,0,0,0,0,0,0,0,0,0,0
