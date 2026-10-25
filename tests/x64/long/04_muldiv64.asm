; EXPECT: pass
; NATIVE: yes
; DESC: 64-bit mul/imul/div/idiv (rdx:rax)
%include "inc/long.inc"

test_main:
    mov rax, 0xFFFFFFFFFFFFFFFF
    mov rcx, 0xFFFFFFFFFFFFFFFF
    mul rcx                                     ; (2^64-1)^2 = 0xFFFFFFFFFFFFFFFE_0000000000000001
    CHECK_CC 3, c
    CHECK_CC 4, o
    CHECK_EQ 1, rax, 1
    CHECK_EQ 2, rdx, 0xFFFFFFFFFFFFFFFE
    mov rax, -3
    mov rcx, 5
    imul rcx                                    ; -15 -> rdx:rax sign extended
    CHECK_CC 7, nc
    CHECK_EQ 5, rax, 0xFFFFFFFFFFFFFFF1
    CHECK_EQ 6, rdx, 0xFFFFFFFFFFFFFFFF
    mov rax, 0x4000000000000000
    imul rax, rax, 4                            ; overflow
    CHECK_CC 8, o
    mov rax, 7
    mov rcx, 6
    imul rax, rcx
    CHECK_EQ 9, rax, 42
    mov rax, 100
    xor edx, edx
    mov rcx, 7
    div rcx
    CHECK_EQ 10, rax, 14
    CHECK_EQ 11, rdx, 2
    mov rax, -100
    cqo
    mov rcx, 7
    idiv rcx
    CHECK_EQ 12, rax, 0xFFFFFFFFFFFFFFF2        ; -14
    CHECK_EQ 13, rdx, 0xFFFFFFFFFFFFFFFE        ; -2
    ; 128 / 64 division
    mov rdx, 1
    mov rax, 0
    mov rcx, 0x10
    div rcx                                     ; 2^64 / 16 = 2^60
    CHECK_EQ 14, rax, 0x1000000000000000
    CHECK_EQ 15, rdx, 0
    cqo
    PASS
