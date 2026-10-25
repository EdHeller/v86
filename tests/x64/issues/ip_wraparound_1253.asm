; EXPECT: pass
; ISSUE: copy/v86#1253
; DESC: 16-bit instruction pointer wraps from 0xffff to 0 inside the code segment
%include "inc/real.inc"

test_main:
    ; cs=0x1000: linear 0x1fffe / 0x1ffff hold two nops, 0x10000 (ip=0, i.e. the
    ; wrapped address) holds a far jump to `wrapped`, 0x20000 (ip=0x10000, no
    ; wrap) holds a far jump to `not_wrapped`.
    mov ax, 0x1000
    mov es, ax
    mov byte [es:0xFFFE], 0x90
    mov byte [es:0xFFFF], 0x90
    mov byte [es:0], 0xEA                       ; jmp far 0000:wrapped
    mov word [es:1], wrapped
    mov word [es:3], 0
    mov ax, 0x2000
    mov es, ax
    mov byte [es:0], 0xEA                       ; jmp far 0000:not_wrapped
    mov word [es:1], not_wrapped
    mov word [es:3], 0
    xor ax, ax
    mov es, ax
    jmp 0x1000:0xFFFE
wrapped:
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7000
    PASS
not_wrapped:
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7000
    FAIL16 1
