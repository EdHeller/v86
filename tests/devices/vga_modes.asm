; A boot sector for tools/profile --check-draw: modes 13h, 04h, 12h, 0Dh, 06h and VBE 0x101 in turn, a letter on COM1 at each.
;   nasm -f bin -o boot.bin vga_modes.asm, padded to 1474560 bytes, as --fda, with --mark A=A ... --mark F=F --check-draw
; boot sector: each VGA mode in turn, its memory filled with a pattern, a few seconds each (by the BIOS tick)
bits 16
org 0x7C00
start:
    cli
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7C00
    sti
    mov si, modes
.next:
    lodsw
    cmp ax, 0xFFFF
    je .done
    push si
    cmp ax, 0x100
    jae .vbe
    int 0x10                ; set the mode (ah = 0, al = mode)
    jmp .fill
.vbe:
    mov bx, ax
    or bx, 0x4000           ; linear frame buffer
    mov ax, 0x4F02
    int 0x10
    jmp .wait               ; (the frame buffer is above 1 MiB: left as the BIOS cleared it, then the palette test)
.fill:
    mov ax, 0xA000
    mov es, ax
    xor di, di
    mov cx, 0xFFFF
    xor al, al
.f:
    stosb
    add al, 37              ; a pattern with every bit pattern
    rol al, 1
    loop .f
    ; every plane written (map mask 0xF) once more with a different pattern for the planar modes
    mov dx, 0x3C4
    mov ax, 0x0F02
    out dx, ax
    mov ax, 0xB800
    mov es, ax
    xor di, di
    mov cx, 0x4000
    mov al, 0x5A
.g:
    stosb
    xor al, 0xC3
    inc al
    loop .g
.wait:
    ; the mode's letter on COM1: the harness checks the drawing there
    mov dx, 0x3F8
    mov al, [letter]
    out dx, al
    inc byte [letter]
    mov cx, 40              ; about two seconds of ticks
.t:
    hlt
    loop .t
    pop si
    jmp .next
.done:
    hlt
    jmp .done
letter: db 'A'
modes: dw 0x13, 0x04, 0x12, 0x0D, 0x06, 0x101, 0xFFFF
times 510 - ($ - $$) db 0
dw 0xAA55
