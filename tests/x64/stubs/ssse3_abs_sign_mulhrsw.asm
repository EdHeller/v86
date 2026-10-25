; EXPECT: pass
; DESC: SSSE3 PABSB/W/D, PSIGNB/D, PMULHRSW (0F 38 08..0B, 1C..1E)
; ISSUE: copy/v86#1038
%include "inc/real.inc"

test_main:
    SSE_ON
    pabsb xmm0, [babs]
    CHECK_XMM 1, xmm0, 0x00058001, 0, 0, 0
    pabsw xmm0, [wabs]
    CHECK_XMM 2, xmm0, 0x80000001, 7, 0, 0
    pabsd xmm0, [dabs]
    CHECK_XMM 3, xmm0, 5, 0x80000000, 9, 1
    movdqa xmm0, [bs_dst]
    psignb xmm0, [bs_src]
    CHECK_XMM 4, xmm0, 0x2800EC0A, 0, 0, 0
    movdqa xmm0, [ds_dst]
    psignd xmm0, [ds_src]
    CHECK_XMM 5, xmm0, 0xFFFFFFFB, 0, 7, 0xFFFFFFF8
    movdqa xmm0, [mh_a]
    pmulhrsw xmm0, [mh_b]
    CHECK_XMM 6, xmm0, 0x80002000, 0x00007FFE, 0, 0
    PASS
align 16
babs: db -1,-128,5,0,0,0,0,0,0,0,0,0,0,0,0,0
wabs: dw -1,-32768,7,0,0,0,0,0
dabs: dd -5,0x80000000,9,-1
bs_dst: db 10,20,30,40,0,0,0,0,0,0,0,0,0,0,0,0
bs_src: db 1,-1,0,5,0,0,0,0,0,0,0,0,0,0,0,0
ds_dst: dd 5,6,7,8
ds_src: dd -1,0,3,-3
mh_a: dw 0x4000,0x8000,0x7FFF,0,0,0,0,0
mh_b: dw 0x4000,0x8000,0x7FFF,0,0,0,0,0
