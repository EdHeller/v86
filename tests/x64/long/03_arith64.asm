; EXPECT: xfail
; NATIVE: yes
; DESC: 64-bit add/sub/adc/sbb/neg/inc/dec with flags
%include "inc/long.inc"

test_main:
    mov rax, 0xFFFFFFFFFFFFFFFF
    add rax, 1
    CHECK_CC 2, z
    CHECK_CC 3, c
    CHECK_EQ 1, rax, 0
    mov rax, 0x7FFFFFFFFFFFFFFF
    add rax, 1
    CHECK_CC 4, o
    CHECK_CC 5, s
    CHECK_CC 6, nc
    mov rax, 0
    sub rax, 1
    CHECK_CC 8, c
    CHECK_CC 9, s
    CHECK_EQ 7, rax, 0xFFFFFFFFFFFFFFFF
    mov rax, 0x8000000000000000
    sub rax, 1
    CHECK_CC 10, o
    ; adc/sbb with carry chain: 128-bit add (2^64-1 + 1) -> 1:0
    mov rax, 0xFFFFFFFFFFFFFFFF
    mov rdx, 0
    add rax, 1
    adc rdx, 0
    CHECK_EQ 11, rax, 0
    CHECK_EQ 12, rdx, 1
    ; 128-bit sub: 1:0 - 1 = 0:ffff...
    sub rax, 1
    sbb rdx, 0
    CHECK_EQ 13, rax, 0xFFFFFFFFFFFFFFFF
    CHECK_EQ 14, rdx, 0
    mov rax, 5
    neg rax
    CHECK_CC 16, c
    CHECK_EQ 15, rax, 0xFFFFFFFFFFFFFFFB
    mov rax, 0x8000000000000000
    neg rax
    CHECK_CC 17, o
    mov rax, 0xFFFFFFFFFFFFFFFF
    inc rax
    CHECK_CC 18, z
    dec rax
    CHECK_CC 19, s
    ; inc/dec leave CF untouched
    stc
    mov rax, 0xFFFFFFFFFFFFFFFF
    inc rax
    CHECK_CC 20, c
    ; imm32 operands are sign extended to 64 bits
    mov rax, 0
    add rax, -1
    CHECK_EQ 21, rax, 0xFFFFFFFFFFFFFFFF
    PASS
