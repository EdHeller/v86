; EXPECT: xfail
; DESC: control registers, cr8/TPR, descriptor table instructions (10-byte sgdt/sidt), ltr/str
%include "inc/long.inc"

test_main:
    mov rax, cr0
    test eax, 1 << 31
    jnz .pg
    mov eax, 1
    xor r15d, r15d
    call report_fail
.pg:
    mov rax, cr3
    CHECK_EQ 2, rax, PML4
    mov rax, cr4
    test eax, 1 << 5
    jnz .pae
    mov eax, 3
    xor r15d, r15d
    call report_fail
.pae:
    mov rax, 5
    mov cr8, rax
    mov rax, 0
    mov rax, cr8
    CHECK_EQ 4, rax, 5
    xor eax, eax
    mov cr8, rax
    sidt [rel idtbuf]
    movzx eax, word [rel idtbuf]
    CHECK_EQ 6, rax, 32 * 16 - 1
    CHECK_EQ 7, qword [rel idtbuf + 2], IDT_BASE      ; full 64-bit base
    sgdt [rel gdtbuf]
    movzx eax, word [rel gdtbuf]
    CHECK_EQ 8, rax, gdt_end - gdt - 1
    ; 64-bit TSS descriptor in the GDT at 0x38 (16 bytes)
    lea rbx, [rel gdt + 0x38]
    lea rax, [rel tss]
    mov word [rbx], 0x67
    mov word [rbx + 2], ax
    shr rax, 16
    mov byte [rbx + 4], al
    mov byte [rbx + 5], 0x89                    ; present, available 64-bit TSS
    mov byte [rbx + 6], 0
    mov byte [rbx + 7], ah
    shr rax, 16
    mov dword [rbx + 8], eax
    mov dword [rbx + 12], 0
    mov ax, 0x38
    ltr ax
    xor eax, eax
    str ax
    CHECK_EQ 9, rax, 0x38
    ; the descriptor was marked busy by ltr
    movzx eax, byte [rbx + 5]
    CHECK_EQ 10, rax, 0x8B
    ; lmsw only changes the low 16 bits
    smsw ax
    test al, 1
    jnz .pe
    mov eax, 11
    xor r15d, r15d
    call report_fail
.pe:
    ; clts, invlpg, wbinvd are privileged but valid at CPL0
    clts
    invlpg [rel idtbuf]
    wbinvd
    PASS
section .data
align 16
idtbuf: times 10 db 0
gdtbuf: times 10 db 0
align 16
tss: times 0x68 db 0
