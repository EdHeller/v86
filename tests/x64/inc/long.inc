; Boots a test into 64-bit long mode and provides exception capture.
;
; Usage in a test:
;     %include "inc/long.inc"
;     test_main:                     ; entered in 64-bit mode, rsp=0x70000
;         ...
;         PASS
;
; Under -DNATIVE (host validation of unprivileged tests) this instead provides
; a Linux _start and an exit-code based PASS/report_fail.
;
; Memory map used by the harness (identity mapped, 2MB pages, first 32MB):
;   0x00600  result block (see harness.inc)
;   0x00620  last exception: vector, error code, rip, count, resume address
;   0x07C00  test image
;   0x800000  IDT (32 * 16); test images may grow up to there (8MB)
;   0x02000  PML4, 0x03000 PDPT, 0x04000 PD (tests may use 0x5000-0x6fff)
;   0x1000000  initial stack top (16MB)

%include "inc/harness.inc"
%include "inc/user.inc"

%define EXC_VEC     0x620
%define EXC_ERR     0x628
%define EXC_RIP     0x630
%define EXC_COUNT   0x638
%define EXC_RESUME  0x640

%define PML4        0x2000
%define PDPT        0x3000
%define PD          0x4000
%define IDT_BASE    0x800000
%define STACK64     0x1000000

%ifdef NATIVE

bits 64
default rel
global _start
section .text
_start:
    jmp test_main

%macro PASS 0
    xor edi, edi
    mov eax, 60
    syscall
%endmacro

report_fail:                ; eax = check id, r15 = actual
    mov edi, eax
    and edi, 0x7F
    or edi, 0x80            ; exit code 128 + (id & 0x7f) so the id is visible
    mov eax, 60
    syscall

%else

bits 16
org 0x7C00

start16:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000
    cld

    ; page tables: identity map 32MB with 2MB pages, user accessible
    mov di, PML4
    xor ax, ax
    mov cx, 3 * 4096 / 2
    rep stosw
    mov dword [PML4], PDPT | 7                  ; present, writable, user
    mov dword [PDPT], PD | 7
    mov di, PD
    mov eax, 0x87
    mov cx, 16
.pd:
    mov [di], eax
    add di, 8
    add eax, 0x200000
    loop .pd

    lgdt [gdt_ptr]

    mov eax, cr4
    or eax, (1 << 5) | (1 << 9) | (1 << 10)     ; PAE, OSFXSR, OSXMMEXCPT
    mov cr4, eax
    mov eax, PML4
    mov cr3, eax

    mov ecx, 0xC0000080                         ; IA32_EFER
    rdmsr
    or eax, (1 << 8) | 1                        ; LME | SCE
    wrmsr

    mov eax, cr0
    and eax, ~(1 << 2)                          ; clear EM
    or eax, (1 << 31) | (1 << 1) | 1            ; PG | MP | PE
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
    mov rsp, STACK64

    ; build the IDT: vectors 0-31 -> isr stubs, rest -> not present
    lea rbx, [rel isr_table]
    mov edi, IDT_BASE
    mov ecx, 32
.idt:
    mov eax, ebx
    mov word [rdi], ax              ; offset 15:0
    mov word [rdi + 2], 0x08        ; selector
    mov word [rdi + 4], 0x8E00      ; present, DPL0, 64-bit interrupt gate
    shr eax, 16
    mov word [rdi + 6], ax          ; offset 31:16
    mov dword [rdi + 8], 0          ; offset 63:32
    mov dword [rdi + 12], 0
    add rbx, 16
    add rdi, 16
    loop .idt
    lidt [rel idt_ptr]

    xor eax, eax
    mov qword [EXC_RESUME], rax
    mov qword [EXC_COUNT], rax
    jmp test_main

; exception stubs ------------------------------------------------------------

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
    push rax
    mov rax, [rsp + 8]
    mov [EXC_VEC], rax
    mov rax, [rsp + 16]
    mov [EXC_ERR], rax
    mov rax, [rsp + 24]
    mov [EXC_RIP], rax
    inc qword [EXC_COUNT]
    mov rax, [EXC_RESUME]
    test rax, rax
    jz .unexpected
    mov [rsp + 24], rax
    pop rax
    add rsp, 16
    iretq
.unexpected:
    mov rax, [EXC_VEC]
    or eax, 0xDEAD0000              ; id = 0xDEAD0000 | vector
    mov r15, [EXC_RIP]
    call report_fail

report_fail:                        ; eax = check id, r15 = actual value
    mov [RESULT_ID], eax
    mov [RESULT_ACT], r15
    mov dword [RESULT], STATUS_FAIL
.halt:
    hlt
    jmp .halt

%macro PASS 0
    mov dword [RESULT], STATUS_PASS
%%halt:
    hlt
    jmp %%halt
%endmacro

align 8
gdt:
    dq 0
    dq 0x00AF9A000000FFFF           ; 0x08 64-bit code, DPL0
    dq 0x00CF92000000FFFF           ; 0x10 data, DPL0
    dq 0x00CF9A000000FFFF           ; 0x18 32-bit code (compatibility mode), DPL0
    dq 0x00CFFA000000FFFF           ; 0x20 32-bit code, DPL3
    dq 0x00CFF2000000FFFF           ; 0x28 data, DPL3
    dq 0x00AFFA000000FFFF           ; 0x30 64-bit code, DPL3
    dq 0, 0                         ; 0x38 room for a 16-byte TSS descriptor (see tests)
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt
idt_ptr:
    dw 32 * 16 - 1
    dq IDT_BASE

; Run code that is expected to raise exception `vec`:
;     TRAP_BEGIN
;         ud2
;     TRAP_END 12, 6          ; check id, vector (-1 would mean no exception)
%macro TRAP_BEGIN 0
    %push trap
    lea rax, [rel %$after]
    mov [EXC_RESUME], rax
    mov qword [EXC_VEC], -1
%endmacro
%macro TRAP_END 2
%$after:
    mov qword [EXC_RESUME], 0
    mov rax, [EXC_VEC]
    CHECK_EQ %1, rax, %2
    %pop
%endmacro

%endif
