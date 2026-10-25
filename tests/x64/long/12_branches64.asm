; EXPECT: xfail
; NATIVE: yes
; DESC: indirect/relative branches in 64-bit mode: jmp/call r64, jmp [mem], jrcxz, loop, movabs moffs
%include "inc/long.inc"

test_main:
    lea rax, [rel t1]
    jmp rax
    mov rax, 1
    CHECK_EQ 1, rax, 0                          ; skipped
t1:
    lea r8, [rel f1]
    call r8
    CHECK_EQ 2, rax, 0x77
    lea rbx, [rel jtab]
    jmp [rbx + 8]
    mov eax, 0
    CHECK_EQ 3, rax, 1
t2:
    xor ecx, ecx
    jrcxz .rcx_zero
    mov eax, 0
    CHECK_EQ 4, rax, 1
.rcx_zero:
    mov rcx, 0x100000000                        ; ecx == 0 but rcx != 0
    jrcxz .bad
    mov rcx, 0x100000000
    db 0x67                                     ; addr32: jecxz
    jrcxz .jecxz_taken
.bad:
    mov eax, 0
    CHECK_EQ 5, rax, 1
.jecxz_taken:
    mov ecx, 5
    xor eax, eax
.l:
    inc rax
    loop .l
    CHECK_EQ 6, rax, 5
    CHECK_EQ 7, rcx, 0
    db 0x48, 0xA1                               ; movabs rax, [moffs64]
    dq data_q
    CHECK_EQ 8, rax, 0xCAFEBABEDEADBEEF
    mov rax, 0x0
    db 0x48, 0xA3                               ; movabs [moffs64], rax
    dq data_w
    mov rax, 0x1
    db 0x48, 0xA1
    dq data_w
    CHECK_EQ 9, rax, 0
    PASS
f1:
    mov eax, 0x77
    ret
align 8
jtab: dq t0, t2
t0:
    mov eax, 0
    CHECK_EQ 10, rax, 1
data_q: dq 0xCAFEBABEDEADBEEF
section .data
data_w: dq 5
