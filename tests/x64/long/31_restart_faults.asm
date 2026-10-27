; EXPECT: pass
; DESC: an instruction whose memory write page faults is restarted by the handler: nothing else may have changed (xadd, xchg, cmpxchg, pop mem, rep stos, ...)
%include "inc/long.inc"

%define PD2 0x5000
%define PT2 0x6000
%define ALIAS 0x200000000
%define PHYS 0x300000

; the page fault handler makes the page writable and retries the instruction
pf_retry:
    or dword [PT2], 2
    push rax
    mov rax, ALIAS
    invlpg [rax]
    pop rax
    inc qword [pf_count]
    add rsp, 8                      ; error code
    iretq

; makes the page read only again, clears the fault counter
%macro RO 0
    mov dword [PT2], PHYS | 5
    mov rax, ALIAS
    invlpg [rax]
    mov qword [pf_count], 0
%endmacro

test_main:
    mov edi, PD2
    xor eax, eax
    mov ecx, 2 * 4096 / 8
    rep stosq
    mov dword [PDPT + 8 * 8], PD2 | 7
    mov dword [PD2], PT2 | 7
    mov dword [PT2], PHYS | 5                   ; read only, user
    mov rax, cr3
    mov cr3, rax
    ; own page fault gate
    lea rax, [rel pf_retry]
    mov edi, IDT_BASE + 14 * 16
    mov word [rdi], ax
    mov word [rdi + 2], 0x08
    mov word [rdi + 4], 0x8E00
    shr eax, 16
    mov word [rdi + 6], ax
    mov dword [rdi + 8], 0
    mov dword [rdi + 12], 0
    mov rax, cr0
    or rax, 1 << 16                             ; WP: supervisor writes to read only pages fault
    mov cr0, rax
    mov rbx, ALIAS

    ; lock xadd: the register must still hold the addend when the instruction is retried
    RO
    mov dword [PHYS], 0x80000001
    mov eax, 0x7fffffff
    lock xadd [rbx], eax
    CHECK_EQ 1, qword [pf_count], 1
    CHECK_EQ32 2, dword [PHYS], 0
    CHECK_EQ 3, rax, 0x80000001

    ; xadd with a REX register
    RO
    mov qword [PHYS], 100
    mov r9, 23
    xadd [rbx], r9
    CHECK_EQ 4, qword [PHYS], 123
    CHECK_EQ 5, r9, 100

    ; xchg
    RO
    mov dword [PHYS], 7
    mov eax, 5
    xchg [rbx], eax
    CHECK_EQ32 6, dword [PHYS], 5
    CHECK_EQ 7, rax, 7
    CHECK_EQ 8, qword [pf_count], 1

    ; cmpxchg that succeeds
    RO
    mov dword [PHYS], 7
    mov eax, 7
    mov ecx, 9
    lock cmpxchg [rbx], ecx
    CHECK_EQ32 9, dword [PHYS], 9
    CHECK_EQ 10, rax, 7
    CHECK_CC 11, z

    ; read-modify-write alu instructions
    RO
    mov dword [PHYS], 10
    mov eax, 3
    add [rbx], eax
    CHECK_EQ32 12, dword [PHYS], 13
    RO
    mov dword [PHYS], 5
    inc dword [rbx]
    CHECK_EQ32 13, dword [PHYS], 6
    RO
    mov dword [PHYS], 0
    bts dword [rbx], 3
    CHECK_EQ32 14, dword [PHYS], 8
    RO
    mov dword [PHYS], 0xF0
    not dword [rbx]
    CHECK_EQ32 15, dword [PHYS], 0xFFFFFF0F

    ; pop to memory: rsp is only advanced when the write has happened
    RO
    mov rdx, rsp
    push qword 0x1234
    pop qword [rbx]
    CHECK_EQ 16, qword [PHYS], 0x1234
    CHECK_EQ 17, rsp, rdx
    CHECK_EQ 18, qword [pf_count], 1

    ; rep stos: the writes before the fault are redone, rdi / rcx end up right
    RO
    mov rdi, ALIAS + 0x100
    mov ecx, 4
    mov eax, 0xAABBCCDD
    rep stosd
    CHECK_EQ 19, rcx, 0
    mov rdx, ALIAS + 0x110
    CHECK_EQ 20, rdi, rdx
    CHECK_EQ32 21, dword [PHYS + 0x10c], 0xAABBCCDD

    ; mov to memory
    RO
    mov rax, 0x1122334455667788
    mov [rbx + 8], rax
    CHECK_EQ 22, qword [PHYS + 8], 0x1122334455667788
    PASS

pf_count: dq 0
