; EXPECT: xfail
; DESC: opcodes that are invalid in 64-bit mode raise #UD (push es, pusha, daa, into, bound/les/lds, aam...)
%include "inc/long.inc"

test_main:
%macro UD_BYTES 2+          ; check id, bytes...
    TRAP_BEGIN
        db %2
        times 8 nop
    TRAP_END %1, 6
%endmacro
    UD_BYTES 0x01, 0x06                         ; push es
    UD_BYTES 0x02, 0x07                         ; pop es
    UD_BYTES 0x03, 0x0E                         ; push cs
    UD_BYTES 0x04, 0x16                         ; push ss
    UD_BYTES 0x05, 0x17                         ; pop ss
    UD_BYTES 0x06, 0x1E                         ; push ds
    UD_BYTES 0x07, 0x1F                         ; pop ds
    UD_BYTES 0x08, 0x27                         ; daa
    UD_BYTES 0x09, 0x2F                         ; das
    UD_BYTES 0x0A, 0x37                         ; aaa
    UD_BYTES 0x0B, 0x3F                         ; aas
    UD_BYTES 0x0C, 0x60                         ; pusha
    UD_BYTES 0x0D, 0x61                         ; popa
    UD_BYTES 0x0E, 0x9A, 0, 0, 0, 0, 0, 0       ; call far ptr16:32
    UD_BYTES 0x0F, 0xCE                         ; into
    UD_BYTES 0x10, 0xD4, 0x0A                   ; aam
    UD_BYTES 0x11, 0xD5, 0x0A                   ; aad
    UD_BYTES 0x12, 0xD6                         ; salc
    UD_BYTES 0x13, 0xEA, 0, 0, 0, 0, 0, 0       ; jmp far ptr16:32
    UD_BYTES 0x14, 0x82, 0xC0, 0x01             ; add al, 1 (alias of 80 /0, invalid in 64-bit)
    UD_BYTES 0x15, 0xC4, 0xE1, 0x78, 0x77       ; VEX prefix (vzeroupper) without AVX enabled
    UD_BYTES 0x16, 0x0F, 0x0B                   ; ud2
    UD_BYTES 0x17, 0x0F, 0x24, 0xC0             ; mov r, tr (removed)
    UD_BYTES 0x18, 0x0F, 0xA7                   ; undefined 0F A7
    ; sysenter/sysexit are valid in long mode on Intel only, so not checked here.
    PASS
