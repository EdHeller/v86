; The VMware backdoor's pointer from a boot sector: GETVERSION, the absolute pointer enabled (its READ_ID), absolute
; mode, then (after "W" on COM1, when the harness moves the host mouse) the status and the packet. Values on COM1 as
; 8 hex digits and a space, "Z" at the end. Run by tests/devices/vmware.js.
bits 16
org 0x7C00
    cli
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7C00

    mov cx, 10              ; GETVERSION
    call door
    call hex                ; 00000006
    mov eax, ebx
    call hex                ; 564D5868
    mov cx, 41
    mov ebx, 0x45414552     ; ENABLE
    call door
    mov cx, 40
    call door
    call hex                ; 00000001
    mov cx, 39
    mov ebx, 1
    call door
    call hex                ; 3442554A
    mov cx, 41
    mov ebx, 0x53424152     ; ABSOLUTE
    call door
    mov dx, 0x3F8
    mov al, 'W'
    out dx, al
.wait:
    mov cx, 40
    call door
    cmp eax, 4
    jb .wait
    call hex                ; 00000004
    mov cx, 39
    mov ebx, 4
    call door
    push ecx
    push ebx
    call hex                ; buttons 00000020 (left held)
    pop eax
    call hex                ; x 00004000 (a quarter)
    pop eax
    call hex                ; y 0000BFFF (three quarters of 0xFFFF)
    mov dx, 0x3F8
    mov al, 'Z'
    out dx, al
.h:
    hlt
    jmp .h

; the backdoor command cx (ebx its argument): eax back
door:
    push dx
    mov eax, 0x564D5868
    and ecx, 0xFFFF
    mov dx, 0x5658
    in eax, dx
    pop dx
    ret

hex:
    push cx
    push ebx
    mov ebx, eax
    mov cx, 8
    mov dx, 0x3F8
.d:
    rol ebx, 4
    mov al, bl
    and al, 0xF
    add al, '0'
    cmp al, '9'
    jbe .o
    add al, 7
.o:
    out dx, al
    loop .d
    mov al, ' '
    out dx, al
    pop ebx
    pop cx
    ret

times 510 - ($ - $$) db 0
dw 0xAA55
