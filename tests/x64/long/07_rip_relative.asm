; EXPECT: xfail
; NATIVE: yes
; DESC: RIP-relative addressing, lea, 64-bit addressing modes, SIB, 32-bit address size override
%include "inc/long.inc"

test_main:
    lea rax, [rel data_q]
    mov rbx, [rel data_q]
    CHECK_EQ 1, rbx, 0xCAFEBABEDEADBEEF
    mov rbx, [rax]
    CHECK_EQ 2, rbx, 0xCAFEBABEDEADBEEF
    mov rcx, 1
    mov rbx, [rax + rcx * 8 - 8]
    CHECK_EQ 3, rbx, 0xCAFEBABEDEADBEEF
    mov rcx, 1
    mov rbx, [rax + rcx * 8]
    CHECK_EQ 4, rbx, 0x0123456789ABCDEF
    lea rdx, [rcx * 4 + 0x10]
    CHECK_EQ 5, rdx, 0x14
    mov r10, rax
    mov r11, 2
    mov rbx, [r10 + r11 * 8 - 16]
    CHECK_EQ 6, rbx, 0xCAFEBABEDEADBEEF
    mov rdx, 0x100000000
    lea rdx, [rax + rdx]                        ; effective address is 64 bits wide
    sub rdx, rax
    CHECK_EQ 7, rdx, 0x100000000
    ; 32-bit address size override truncates the effective address
    mov rsi, rax
    mov rdx, 0x100000000
    or rsi, rdx
    mov rbx, [esi]                              ; 0x67 prefix: address is truncated to 32 bits
    CHECK_EQ 8, rbx, 0xCAFEBABEDEADBEEF
    PASS
align 8
data_q: dq 0xCAFEBABEDEADBEEF, 0x0123456789ABCDEF
