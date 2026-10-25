; Minimal boot stub for tests/x64/manual/linux64-direct.js: enters long mode with an identity
; mapping of the first 1GB (2MB pages), loads rsi with the boot_params address and jumps to the
; 64-bit entry point of a kernel that was loaded by the script (the "64-bit boot protocol").
;
; The script patches the two qwords at `params` and `entry` before running.

bits 16
org 0x7C00

PML4 equ 0x2000
PDPT equ 0x3000
PD   equ 0x4000

start:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000
    cld

    mov di, PML4
    mov cx, 3 * 4096 / 2
    rep stosw
    mov dword [PML4], PDPT | 3
    mov dword [PDPT], PD | 3
    mov di, PD
    mov eax, 0x83
    mov cx, 512
.pd:
    mov [di], eax
    add di, 8
    add eax, 0x200000
    loop .pd

    lgdt [gdt_ptr]
    mov eax, cr4
    or eax, (1 << 5)                    ; PAE
    mov cr4, eax
    mov eax, PML4
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8                      ; LME
    wrmsr
    mov eax, cr0
    or eax, (1 << 31) | 1
    mov cr0, eax
    jmp dword 0x08:start64

bits 64
start64:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor eax, eax
    mov fs, ax
    mov gs, ax
    mov rsp, 0x7000
    mov rsi, [rel params]
    mov rax, [rel entry]
    jmp rax

align 8
gdt:
    dq 0
    dq 0x00AF9A000000FFFF
    dq 0x00CF92000000FFFF
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt
align 8
params: dq 0
entry:  dq 0
