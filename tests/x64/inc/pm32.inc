; 32-bit flat protected mode (no paging) test environment.
;
;     %include "inc/pm32.inc"
;     test_main:                     ; 32-bit, flat, esp = 0x70000
;         TRAP_BEGIN / TRAP_END as in inc/real.inc (32-bit variants)
;         PASS
;
; Selectors: 0x08 code32 DPL0, 0x10 data DPL0. IDT vectors 0-31 are installed;
; recorded state uses the same EXC_* addresses as inc/real.inc.

%include "inc/harness.inc"

%define EXC_VEC     0x620
%define EXC_ERR     0x628
%define EXC_RIP     0x630
%define EXC_COUNT   0x638
%define EXC_RESUME  0x640
%define IDT_BASE    0x20000
%define STACK32     0x70000

bits 16
org 0x7C00

start16:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000
    lgdt [gdt_ptr]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp dword 0x08:start32

bits 32
start32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    xor eax, eax
    mov fs, ax
    mov gs, ax
    mov esp, STACK32
    cld

    mov ebx, isr_table
    mov edi, IDT_BASE
    mov ecx, 32
.idt:
    mov eax, ebx
    mov word [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov word [edi + 6], ax
    add ebx, 16
    add edi, 8
    loop .idt
    lidt [idt_ptr]
    mov dword [EXC_RESUME], 0
    mov dword [EXC_COUNT], 0
    mov dword [EXC_VEC], -1
    jmp test_main

%define ERRCODE_VECTORS ((1<<8)|(1<<10)|(1<<11)|(1<<12)|(1<<13)|(1<<14)|(1<<17)|(1<<21)|(1<<29)|(1<<30))

align 16
isr_table:
%assign v 0
%rep 32
align 16
%if (ERRCODE_VECTORS >> v) & 1
    push v
    jmp exc_common
%else
    push 0
    push v
    jmp exc_common
%endif
%assign v v+1
%endrep

exc_common:
    push eax
    mov eax, [esp + 4]
    mov [EXC_VEC], eax
    mov eax, [esp + 8]
    mov [EXC_ERR], eax
    mov eax, [esp + 12]
    mov [EXC_RIP], eax
    inc dword [EXC_COUNT]
    mov eax, [EXC_RESUME]
    test eax, eax
    jz .unexpected
    mov [esp + 12], eax
    pop eax
    add esp, 8
    iretd
.unexpected:
    mov eax, [EXC_VEC]
    or eax, 0xDEAD0000
    mov [RESULT_ID], eax
    mov eax, [EXC_RIP]
    mov [RESULT_ACT], eax
    mov dword [RESULT], STATUS_FAIL
    hlt
    jmp $-1

%macro TRAP_BEGIN 0
    %push trap
    mov dword [EXC_RESUME], %$after
    mov dword [EXC_VEC], -1
%endmacro
%macro TRAP_END 2
%$after:
    mov dword [EXC_RESUME], 0
    cmp dword [EXC_VEC], %2
    je %%ok
    FAIL16 %1
%%ok:
    %pop
%endmacro

%macro PASS 0
    PASS_REAL
%endmacro

align 8
gdt:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt
idt_ptr:
    dw 32 * 8 - 1
    dd IDT_BASE
