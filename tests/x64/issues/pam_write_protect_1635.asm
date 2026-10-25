; EXPECT: xfail
; ISSUE: copy/v86#1635
; DESC: i440FX PAM registers: a read-only shadow region (PAM=01) ignores writes (Windows 98 corrupts the VGA BIOS otherwise)
%include "inc/real.inc"

; PAM1 (config offset 0x5A) controls 0xC0000-0xC7FFF: bits 1:0 = C0000-C3FFF,
; bits 5:4 = C4000-C7FFF; 00 disabled, 01 read-only, 10 write-only, 11 read/write.
%macro SET_PAM1 1
    mov dx, 0xCF8
    mov eax, 0x80000058                         ; bus 0, dev 0, fn 0, dword containing 0x58..0x5B
    out dx, eax
    mov dx, 0xCFC
    in eax, dx
    and eax, 0xFF00FFFF
    or eax, %1 << 16
    out dx, eax
%endmacro

test_main:
    mov ax, 0xC000
    mov es, ax
    SET_PAM1 0x33                               ; read/write
    mov byte [es:0x100], 0xA5
    CHECK16 1, byte [es:0x100], 0xA5            ; reads see the write
    SET_PAM1 0x11                               ; read-only: reads come from RAM, writes are dropped
    CHECK16 2, byte [es:0x100], 0xA5
    mov byte [es:0x100], 0x5A
    CHECK16 3, byte [es:0x100], 0xA5            ; unchanged
    mov word [es:0x102], 0xFFFF
    CHECK16 4, word [es:0x102], 0
    SET_PAM1 0x33
    mov byte [es:0x100], 0x5A
    CHECK16 5, byte [es:0x100], 0x5A            ; writable again
    PASS
