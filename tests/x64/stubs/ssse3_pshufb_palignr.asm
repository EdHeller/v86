; EXPECT: xfail
; DESC: SSSE3 PSHUFB (xmm and mmx), PALIGNR (0F 38 00 / 0F 3A 0F stubs)
; ISSUE: copy/v86#1038
%include "inc/real.inc"

test_main:
    SSE_ON
    movdqa xmm0, [a]
    movdqa xmm1, [rev]
    pshufb xmm0, xmm1
    CHECK_XMM 1, xmm0, 0x0C0D0E0F, 0x08090A0B, 0x04050607, 0x00010203
    movdqa xmm0, [a]
    movdqa xmm1, [zero_mask]
    pshufb xmm0, xmm1                           ; bit 7 set in the control byte zeroes the byte
    CHECK_XMM 2, xmm0, 0, 0, 0, 0
    movdqa xmm0, [a]
    pshufb xmm0, [rev]                          ; memory operand form
    CHECK_XMM 3, xmm0, 0x0C0D0E0F, 0x08090A0B, 0x04050607, 0x00010203
    movq mm0, [a]
    movq mm1, [rev + 8]                         ; bytes 7..0
    pshufb mm0, mm1
    movq [TMP], mm0
    emms
    CHECK16 4, dword [TMP], 0x04050607
    CHECK16 5, dword [TMP + 4], 0x00010203
    movdqa xmm0, [a]                            ; dst
    movdqa xmm1, [b]                            ; src
    palignr xmm0, xmm1, 4
    CHECK_XMM 6, xmm0, 0x17161514, 0x1B1A1918, 0x1F1E1D1C, 0x03020100
    movdqa xmm0, [a]
    palignr xmm0, xmm1, 16                      ; shifts out all of src: result is dst
    CHECK_XMM 7, xmm0, 0x03020100, 0x07060504, 0x0B0A0908, 0x0F0E0D0C
    movdqa xmm0, [a]
    palignr xmm0, xmm1, 32                      ; shifts out everything
    CHECK_XMM 8, xmm0, 0, 0, 0, 0
    PASS
align 16
a:         db 0,1,2,3,4,5,6,7,8,9,10,11,12,13,14,15
b:         db 16,17,18,19,20,21,22,23,24,25,26,27,28,29,30,31
rev:       db 15,14,13,12,11,10,9,8,7,6,5,4,3,2,1,0
zero_mask: times 16 db 0x80
