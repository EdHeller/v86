; EXPECT: pass
; NATIVE: yes
; DESC: REX extended registers r8-r15 and byte registers spl/bpl/sil/dil
%include "inc/long.inc"

test_main:
    mov r8, 8
    mov r9, 9
    mov r10, 10
    mov r11, 11
    mov r12, 12
    mov r13, 13
    mov r14, 14
    xor eax, eax
    add rax, r8
    add rax, r9
    add rax, r10
    add rax, r11
    add rax, r12
    add rax, r13
    add rax, r14
    CHECK_EQ 1, rax, 8 + 9 + 10 + 11 + 12 + 13 + 14
    mov rsi, -1
    mov sil, 0x12
    CHECK_EQ 2, rsi, 0xFFFFFFFFFFFFFF12
    mov rdi, -1
    mov dil, 0x34
    CHECK_EQ 3, rdi, 0xFFFFFFFFFFFFFF34
    mov rbp, -1
    mov bpl, 0x56
    CHECK_EQ 4, rbp, 0xFFFFFFFFFFFFFF56
    mov r12, -1
    mov r12b, 0x78
    CHECK_EQ 5, r12, 0xFFFFFFFFFFFFFF78
    mov r13, -1
    mov r13w, 0x9ABC
    CHECK_EQ 6, r13, 0xFFFFFFFFFFFF9ABC
    mov r14d, 0x11223344
    CHECK_EQ 7, r14, 0x11223344
    mov r15, rsp
    mov spl, 0
    mov rsp, r15
    PASS
