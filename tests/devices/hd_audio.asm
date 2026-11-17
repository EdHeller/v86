; HD Audio (audio_device "hd_audio") from a boot floppy: the controller found on PCI bus 0 (8086:2668), out of
; reset, the codec's vendor ID by the immediate command, the AFG's node count through the CORB and RIRB, the DAC
; set to stream 1 at full gain, then stream 0 plays a 2-entry list of 256 bytes each (a ramp) at 48 kHz, 16-bit
; stereo, until its first completion interrupt; the DMA position buffer then. Results on COM1 as 8 hex digits and
; a space, "Z" at the end. Run by tests/devices/hd_audio.js.
bits 16
org 0x7C00
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7C00
    ; the rest of the program: 3 more sectors after this one
    mov ax, 0x0203
    mov bx, 0x7E00
    mov cx, 0x0002
    xor dh, dh
    int 0x13
    lgdt [gdt_desc]
    mov eax, cr0
    or al, 1
    mov cr0, eax
    jmp 0x08:pm

gdt:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
gdt_desc:
    dw gdt_desc - gdt - 1
    dd gdt

times 510 - ($ - $$) db 0
dw 0xAA55

bits 32
pm:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x7C00

    ; find 8086:2668 on bus 0
    xor ebx, ebx
.scan:
    mov eax, ebx
    shl eax, 11
    or eax, 0x80000000
    mov [cfg], eax
    mov dx, 0xCF8
    out dx, eax
    mov dx, 0xCFC
    in eax, dx
    cmp eax, 0x26688086
    je .found
    inc ebx
    cmp ebx, 32
    jb .scan
    mov eax, 0xDEAD0000
    call hex
    jmp done
.found:
    ; memory space and bus master
    mov eax, [cfg]
    or eax, 4
    mov dx, 0xCF8
    out dx, eax
    mov dx, 0xCFC
    in ax, dx
    or ax, 6
    out dx, ax              ; (16 bits: v86 ignores 32-bit writes of the command register)
    mov eax, [cfg]
    or eax, 0x10
    mov dx, 0xCF8
    out dx, eax
    mov dx, 0xCFC
    in eax, dx
    and eax, ~0xF
    mov esi, eax            ; BAR0

    ; out of reset; the codec asks for its address
    mov dword [esi + 0x08], 1
    mov ecx, 100000
.crst:
    test byte [esi + 0x08], 1
    loopz .crst
    movzx eax, word [esi + 0x0E]     ; STATESTS: 1
    call hex
    movzx eax, word [esi + 0x00]     ; GCAP: 0x1001
    call hex

    ; the immediate command: the root's vendor ID
    mov dword [esi + 0x60], 0x000F0000
    mov word [esi + 0x68], 1
.icis:
    test byte [esi + 0x68], 2
    jz .icis
    mov eax, [esi + 0x64]           ; 1AF40012
    call hex
    mov word [esi + 0x68], 2        ; IRV cleared

    ; CORB at 0x10000, RIRB at 0x11000
    mov dword [esi + 0x40], 0x10000
    mov dword [esi + 0x44], 0
    mov dword [esi + 0x50], 0x11000
    mov dword [esi + 0x54], 0
    mov word [esi + 0x4A], 0x8000   ; CORBRP reset
    mov word [esi + 0x4A], 0
    mov word [esi + 0x58], 0x8000   ; RIRBWP reset
    mov word [esi + 0x5A], 1        ; RINTCNT
    mov byte [esi + 0x5C], 3        ; RIRB DMA, its interrupt
    mov byte [esi + 0x4C], 2        ; CORB runs
    mov dword [0x10004], 0x001F0004 ; AFG: node count
    mov dword [0x10008], 0x00270610 ; DAC: stream 1, channel 0
    mov dword [0x1000C], 0x0023B04A ; DAC: output amplifier, both sides, 0 dB
    mov dword [0x10010], 0x002B8000 ; DAC: its left gain (4A)
    mov word [esi + 0x48], 4
.rirb:
    cmp word [esi + 0x58], 4
    jne .rirb
    mov eax, [0x11008]              ; 00020002
    call hex
    mov eax, [0x11020]              ; 0000004A
    call hex
    movzx eax, byte [esi + 0x5D]    ; RIRBSTS: RINTFL
    call hex
    mov byte [esi + 0x5D], 1
    movzx eax, word [esi + 0x4A]    ; CORBRP 4
    call hex

    ; the samples: a ramp of 16-bit words at 0x13000
    mov edi, 0x13000
    xor eax, eax
    mov ecx, 256
.fill:
    stosw
    add ax, 0x0101
    loop .fill
    ; the list: two entries of 256 bytes, both with IOC
    mov dword [0x12000], 0x13000
    mov dword [0x12004], 0
    mov dword [0x12008], 256
    mov dword [0x1200C], 1
    mov dword [0x12010], 0x13100
    mov dword [0x12014], 0
    mov dword [0x12018], 256
    mov dword [0x1201C], 1
    mov dword [esi + 0x70], 0x14001 ; the DMA position buffer
    mov dword [esi + 0x74], 0
    mov dword [esi + 0x80 + 0x18], 0x12000
    mov dword [esi + 0x80 + 0x1C], 0
    mov dword [esi + 0x80 + 0x08], 512
    mov word [esi + 0x80 + 0x0C], 1
    mov word [esi + 0x80 + 0x12], 0x0011
    mov dword [esi + 0x20], 0x80000001  ; INTCTL: GIE, stream 0
    mov dword [esi + 0x80], 0x00100006  ; tag 1, IOCE, RUN
.bcis:
    test byte [esi + 0x83], 4
    jz .bcis
    mov eax, [esi + 0x24]           ; INTSTS: 80000001
    call hex
    movzx eax, byte [esi + 0x83]    ; SDSTS: BCIS, FIFORDY (24)
    call hex
    mov byte [esi + 0x83], 4
    mov eax, [esi + 0x24]           ; 0
    call hex
    mov dword [esi + 0x80], 0x00100000  ; stopped
    movzx eax, byte [esi + 0x83]    ; FIFORDY gone: 0
    and eax, 0x20
    call hex
    mov eax, [esi + 0x0C + 0x80 - 0x0C + 4] ; LPIB, a multiple of 4
    and eax, 3
    call hex
    mov eax, [0x14000]              ; the position buffer as LPIB
    cmp eax, [esi + 0x84]
    sete al
    movzx eax, al
    call hex
done:
    mov dx, 0x3F8
    mov al, 'Z'
    out dx, al
.h:
    hlt
    jmp .h

; eax as 8 hex digits and a space
hex:
    mov ecx, 8
    mov ebx, eax
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
    ret

cfg: dd 0
times 2048 - ($ - $$) db 0
