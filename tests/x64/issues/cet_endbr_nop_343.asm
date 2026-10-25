; EXPECT: pass
; ISSUE: copy/v86#343
; DESC: CET instructions execute as NOPs when CET is not enabled: ENDBR32/64, RDSSPD, hint-NOP 0F 1E /r
%include "inc/real.inc"

test_main:
    mov eax, 0x12345678
    db 0xF3, 0x0F, 0x1E, 0xFB                   ; endbr32
    db 0xF3, 0x0F, 0x1E, 0xFA                   ; endbr64
    db 0xF3, 0x0F, 0x1E, 0xC8                   ; rdsspd eax (reads nothing, leaves eax) when CR4.CET=0
    CHECK16 1, eax, 0x12345678
    db 0x0F, 0x1E, 0xC0                         ; nop eax (hint nop, no F3 prefix)
    db 0x0F, 0x1E, 0x06                         ; nop word [addr]
    dw TMP
    db 0x0F, 0x1F, 0x00                         ; nop [eax-less addr]  (0F 1F /0)
    db 0x0F, 0x19, 0xC0                         ; reserved-nop 0F 19..1D
    db 0x0F, 0x18, 0x06                         ; prefetchnta [..]
    dw TMP
    CHECK16 2, dword [EXC_VEC], -1
    PASS
