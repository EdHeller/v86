; EXPECT: pass
; NATIVE: yes
; DESC: cmpxchg16b with REX.W, lock prefix
%include "inc/long.inc"

test_main:
    lea rsi, [rel q]
    mov rax, 0x1111111111111111
    mov rdx, 0x2222222222222222
    mov rbx, 0xAAAAAAAAAAAAAAAA
    mov rcx, 0xBBBBBBBBBBBBBBBB
    lock cmpxchg16b [rsi]
    CHECK_CC 1, z
    CHECK_EQ 2, [rsi], 0xAAAAAAAAAAAAAAAA
    CHECK_EQ 3, [rsi + 8], 0xBBBBBBBBBBBBBBBB
    cmpxchg16b [rsi]                            ; now mismatches: rdx:rax <- mem
    CHECK_CC 4, nz
    CHECK_EQ 5, rax, 0xAAAAAAAAAAAAAAAA
    CHECK_EQ 6, rdx, 0xBBBBBBBBBBBBBBBB
    PASS
section .data
align 16
q: dq 0x1111111111111111, 0x2222222222222222
