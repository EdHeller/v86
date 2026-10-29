; EXPECT: pass
; V86: {"acpi": true, "cpu_cores": 2, "cpu_workers": true, "vcpu_worker_url": "src/parallel/vcpu_worker.js", "wasm_path": "build/v86-parallel-debug.wasm"}
; DESC: LOCKed read-modify-write instructions in long mode are atomic between a core in a worker and the boot core (lock inc/xadd/cmpxchg/bts, xchg), also while jit64 compiles the loops
%include "inc/long.inc"

%define AP_DONE    0x800
%define COUNT_INC  0x810
%define COUNT_ADD  0x814
%define COUNT_LOCKED 0x818
%define SPIN_LOCK  0x81C
%define COUNT_BTS  0x820
%define COUNT_XCHG 0x830
%define ROUNDS     30000

%macro WORK 0
    mov ecx, ROUNDS
%%loop:
    lock inc dword [COUNT_INC]
    mov eax, 3
    lock xadd [COUNT_ADD], eax
%%acquire:
    xor eax, eax
    mov edx, 1
    lock cmpxchg [SPIN_LOCK], edx
    jnz %%acquire
    mov eax, [COUNT_LOCKED]
    inc eax
    mov [COUNT_LOCKED], eax
    mov dword [SPIN_LOCK], 0
%%bts:
    lock bts dword [COUNT_BTS], 0
    jc %%bts
    mov eax, [COUNT_BTS + 4]
    inc eax
    mov [COUNT_BTS + 4], eax
    mov dword [COUNT_BTS], 0
%%xchg:
    mov eax, 1
    xchg [COUNT_XCHG], eax
    test eax, eax
    jnz %%xchg
    mov eax, [COUNT_XCHG + 4]
    inc eax
    mov [COUNT_XCHG + 4], eax
    mov dword [COUNT_XCHG], 0
    dec ecx
    jnz %%loop
%endmacro

test_main:
    ; map the APIC page (0xFEE00000) too: PDPT[3] -> a page directory with one 2 MB uncached page
    mov rdi, 0x5000
    xor eax, eax
    mov ecx, 512
    rep stosq
    mov qword [0x3000 + 3 * 8], 0x5000 | 7
    mov eax, 0xFEE00000 | 0x97
    mov [0x5000 + 503 * 8], rax
    mov rax, cr3
    mov cr3, rax
    xor eax, eax
    mov rdi, 0x800
    mov ecx, 16
    rep stosd
    lea rsi, [rel ap_src]
    mov edi, 0x9000
    mov ecx, ap_end - ap_entry
    cld
    rep movsb
    mov ebx, 0xFEE00300
    mov dword [rbx + 0x10], 1 << 24
    mov dword [rbx], 0x0000C500
    mov dword [rbx], 0x00008500
    mov dword [rbx], 0x00000609
    WORK
    mov rcx, 4000000000
.wait:
    cmp dword [AP_DONE], 1
    je .done
    dec rcx
    jnz .wait
    mov eax, 1
    xor r15, r15
    call report_fail
.done:
    mov eax, 2
    mov r15d, [COUNT_INC]
    cmp r15d, 2 * ROUNDS
    jne report_fail
    mov eax, 3
    mov r15d, [COUNT_ADD]
    cmp r15d, 6 * ROUNDS
    jne report_fail
    mov eax, 4
    mov r15d, [COUNT_LOCKED]
    cmp r15d, 2 * ROUNDS
    jne report_fail
    mov eax, 5
    mov r15d, [COUNT_BTS + 4]
    cmp r15d, 2 * ROUNDS
    jne report_fail
    mov eax, 6
    mov r15d, [COUNT_XCHG + 4]
    cmp r15d, 2 * ROUNDS
    jne report_fail
    PASS

ap_src:
section .ap vstart=0x9000 follows=.text align=1
bits 16
ap_entry:
    cli
    xor ax, ax
    mov ds, ax
    lgdt [gdt_ptr]
    mov eax, cr4
    or eax, (1 << 5) | (1 << 9) | (1 << 10)
    mov cr4, eax
    mov eax, PML4
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8) | 1
    wrmsr
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, (1 << 31) | (1 << 1) | 1
    mov cr0, eax
    jmp dword 0x08:ap64
bits 64
ap64:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0x1800000
    WORK
    mov dword [AP_DONE], 1
.halt:
    sti
    hlt
    jmp .halt
ap_end:
