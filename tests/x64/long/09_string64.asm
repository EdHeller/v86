; EXPECT: xfail
; NATIVE: yes
; DESC: REX.W string instructions: movsq/stosq/lodsq/scasq/cmpsq with rep
%include "inc/long.inc"

test_main:
    lea rdi, [rel buf]
    mov rax, 0x1122334455667788
    mov ecx, 4
    cld
    rep stosq
    CHECK_EQ 1, rcx, 0
    lea rax, [rel buf + 32]
    CHECK_EQ 2, rdi, rax
    CHECK_EQ 3, [rel buf + 24], 0x1122334455667788
    lea rsi, [rel buf]
    lea rdi, [rel buf2]
    mov ecx, 4
    rep movsq
    CHECK_EQ 4, [rel buf2 + 8], 0x1122334455667788
    lea rsi, [rel buf]
    lodsq
    CHECK_EQ 5, rax, 0x1122334455667788
    lea rax, [rel buf + 8]
    CHECK_EQ 6, rsi, rax
    lea rdi, [rel buf]
    mov rax, 0x1122334455667788
    mov ecx, 4
    repe scasq
    CHECK_CC 7, z                               ; all four equal -> ZF=1, rcx = 0
    CHECK_EQ 8, rcx, 0
    mov qword [rel buf2 + 16], 0
    lea rsi, [rel buf]
    lea rdi, [rel buf2]
    mov ecx, 4
    repe cmpsq                                  ; third qword differs
    CHECK_CC 9, nz
    CHECK_EQ 10, rcx, 1
    ; backwards copy with DF=1
    std
    lea rsi, [rel buf + 24]
    lea rdi, [rel buf2 + 24]
    mov ecx, 2
    rep movsq
    cld
    lea rax, [rel buf2 + 8]
    CHECK_EQ 11, rdi, rax
    ; 32-bit stos writes only 4 bytes and advances rdi by 4
    lea rdi, [rel buf]
    mov eax, 0
    stosd
    lea rax, [rel buf + 4]
    CHECK_EQ 12, rdi, rax
    CHECK_EQ32 13, dword [rel buf + 4], 0x11223344
    PASS
section .data
align 8
buf:  times 4 dq 0
buf2: times 4 dq 0
