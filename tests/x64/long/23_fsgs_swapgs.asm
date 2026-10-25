; EXPECT: xfail
; DESC: FS/GS segment bases in 64-bit mode, swapgs, rd/wr{fs,gs}base
%include "inc/long.inc"

test_main:
    lea rbx, [rel fsdata]
    mov rax, rbx
    mov rdx, rax
    shr rdx, 32
    mov ecx, 0xC0000100
    wrmsr
    mov rax, [fs:0]
    CHECK_EQ 1, rax, 0xF5F5F5F5F5F5F5F5
    mov rax, [fs:8]
    CHECK_EQ 2, rax, 0x0606060606060606
    lea rbx, [rel gsdata]
    mov rax, rbx
    mov rdx, rax
    shr rdx, 32
    mov ecx, 0xC0000101
    wrmsr
    mov rax, [gs:0]
    CHECK_EQ 3, rax, 0xA5A5A5A5A5A5A5A5
    ; swapgs exchanges GS.base and KernelGSBase
    lea rbx, [rel fsdata]
    mov rax, rbx
    mov rdx, rax
    shr rdx, 32
    mov ecx, 0xC0000102
    wrmsr
    swapgs
    mov rax, [gs:0]
    CHECK_EQ 4, rax, 0xF5F5F5F5F5F5F5F5
    swapgs
    mov rax, [gs:0]
    CHECK_EQ 5, rax, 0xA5A5A5A5A5A5A5A5
    ; segment override on lea/rip-relative uses the base too
    lea rbx, [fs:rbx]                           ; lea ignores the segment
    lea rax, [rel fsdata]
    CHECK_EQ 6, rbx, rax
    ; es/cs/ss/ds overrides are ignored in 64-bit mode
    lea rbx, [rel fsdata]
    mov rax, [ds:rbx]
    CHECK_EQ 7, rax, 0xF5F5F5F5F5F5F5F5
    ; loading a data selector: base stays 0 in long mode, so the access is flat
    mov ax, 0x10
    mov gs, ax
    mov rax, [gs:rbx]
    CHECK_EQ 8, rax, 0xF5F5F5F5F5F5F5F5
    ; wrfsbase/rdfsbase need CR4.FSGSBASE
    mov rax, cr4
    or rax, 1 << 16
    mov cr4, rax
    mov rax, 0x0000123456789ABC
    wrfsbase rax
    rdfsbase rcx
    CHECK_EQ 9, rcx, 0x0000123456789ABC
    wrgsbase rax
    rdgsbase rcx
    CHECK_EQ 10, rcx, 0x0000123456789ABC
    PASS
align 8
fsdata: dq 0xF5F5F5F5F5F5F5F5, 0x0606060606060606
gsdata: dq 0xA5A5A5A5A5A5A5A5
