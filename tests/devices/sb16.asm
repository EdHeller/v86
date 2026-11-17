; A boot sector exercising the Sound Blaster 16: DSP reset, version, the invert and test registers, the copyright
; string, the mixer (a legacy register and its pair, the IRQ and DMA configuration), an 8-bit single-cycle DMA
; playback of 256 bytes 0..255 on channel 1 and its interrupt (mixer 0x82, acknowledged at 0x22E).
; Each result goes to COM1 as two hex digits and a space, then "Z". Run by tests/devices/sb16.js.
bits 16
org 0x7C00
start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00

    ; DSP reset
    mov dx, 0x226
    mov al, 1
    out dx, al
    xor al, al
    out dx, al
    call dsp_read           ; AA

    mov al, 0xE1
    call dsp_write
    call dsp_read           ; 04
    call dsp_read           ; 05

    mov al, 0xE0
    call dsp_write
    mov al, 0x5A
    call dsp_write
    call dsp_read           ; A5

    mov al, 0xE4
    call dsp_write
    mov al, 0x33
    call dsp_write
    mov al, 0xE8
    call dsp_write
    call dsp_read           ; 33

    mov al, 0xE3
    call dsp_write
    call dsp_read           ; 43 'C'
    call dsp_read           ; 4F 'O'

    ; mixer
    mov ah, 0x22
    mov al, 0xA5
    call mixer_write
    mov ah, 0x30
    call mixer_read         ; A0
    mov ah, 0x31
    call mixer_read         ; 50
    mov ah, 0x22
    call mixer_read         ; A5
    mov ah, 0x80
    call mixer_read         ; 02
    mov ah, 0x81
    call mixer_read         ; 22

    ; the samples
    mov di, 0x8000
    xor al, al
    mov cx, 256
.fill:
    stosb
    inc al
    loop .fill

    ; DMA channel 1: memory to device, single, 256 bytes at 0x8000
    mov al, 5
    out 0x0A, al            ; mask
    out 0x0C, al            ; flip-flop
    mov al, 0x49
    out 0x0B, al
    xor al, al
    out 0x02, al
    mov al, 0x80
    out 0x02, al
    xor al, al
    out 0x83, al
    mov al, 0xFF
    out 0x03, al
    xor al, al
    out 0x03, al
    mov al, 1
    out 0x0A, al            ; unmask

    mov al, 0xD1
    call dsp_write
    mov al, 0x40
    call dsp_write
    mov al, 0xA6
    call dsp_write
    mov al, 0x14
    call dsp_write
    mov al, 0xFF
    call dsp_write
    xor al, al
    call dsp_write

    ; wait for the transfer's interrupt (the harness asks for the samples)
.wait:
    mov ah, 0x82
    call mixer_get
    test al, 1
    jz .wait
    call hex                ; 21
    mov dx, 0x22E
    in al, dx               ; acknowledge
    mov ah, 0x82
    call mixer_read         ; 20

    mov dx, 0x3F8
    mov al, 'Z'
    out dx, al
.halt:
    hlt
    jmp .halt

dsp_write:
    push ax
    mov dx, 0x22C
.w:
    in al, dx
    test al, 0x80
    jnz .w
    pop ax
    out dx, al
    ret

dsp_read:
    mov dx, 0x22E
.r:
    in al, dx
    test al, 0x80
    jz .r
    mov dx, 0x22A
    in al, dx
    jmp hex

mixer_write:
    mov dx, 0x224
    xchg al, ah
    out dx, al
    xchg al, ah
    inc dx
    out dx, al
    ret

mixer_get:
    mov dx, 0x224
    mov al, ah
    out dx, al
    inc dx
    in al, dx
    ret

mixer_read:
    call mixer_get
    ; fall through

; al as two hex digits and a space on COM1
hex:
    push ax
    mov dx, 0x3F8
    shr al, 4
    call digit
    pop ax
    and al, 0x0F
    call digit
    mov al, ' '
    out dx, al
    ret
digit:
    add al, '0'
    cmp al, '9'
    jbe .o
    add al, 7
.o:
    out dx, al
    ret

times 510 - ($ - $$) db 0
dw 0xAA55
