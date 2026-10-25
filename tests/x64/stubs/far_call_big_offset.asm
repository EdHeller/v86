; EXPECT: pass
; QEMU: xfail  (qemu does not deliver #DB for ICEBP / does not limit-check far offsets in real mode)
; DESC: far call/jmp with a 32-bit offset above 0xffff in real mode raises #GP instead of being ignored
%include "inc/real.inc"

test_main:
    TRAP_BEGIN
        db 0x66, 0x9A                           ; call far ptr16:32
        dd 0x00010000
        dw 0
    TRAP_END 1, 13
    TRAP_BEGIN
        db 0x66, 0xEA                           ; jmp far ptr16:32
        dd 0x00010000
        dw 0
    TRAP_END 2, 13
    TRAP_BEGIN
        mov dword [farptr], 0x00010000
        mov word [farptr + 4], 0
        db 0x66, 0xFF, 0x1E                     ; call far dword... m16:32
        dw farptr
    TRAP_END 3, 13
    PASS
farptr: dd 0
        dw 0
