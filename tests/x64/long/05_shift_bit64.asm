; EXPECT: xfail
; NATIVE: yes
; DESC: 64-bit shifts and rotates, shld/shrd, bt family, bsf/bsr/popcnt/bswap
%include "inc/long.inc"

test_main:
    mov rax, 1
    shl rax, 63
    CHECK_EQ 1, rax, 0x8000000000000000
    shl rax, 1
    CHECK_CC 3, c
    CHECK_EQ 2, rax, 0
    mov rax, 0x8000000000000000
    sar rax, 63
    CHECK_EQ 4, rax, 0xFFFFFFFFFFFFFFFF
    mov rax, 0x8000000000000000
    shr rax, 63
    CHECK_EQ 5, rax, 1
    mov rax, 0x8000000000000001
    rol rax, 1
    CHECK_EQ 6, rax, 3
    mov rax, 0x8000000000000001
    ror rax, 1
    CHECK_EQ 7, rax, 0xC000000000000000
    mov cl, 68                                  ; count is masked to 6 bits -> 4
    mov rax, 0x10
    shl rax, cl
    CHECK_EQ 8, rax, 0x100
    mov rax, 0x1234567812345678
    mov rdx, 0xFFFFFFFFFFFFFFFF
    shld rax, rdx, 8
    CHECK_EQ 9, rax, 0x34567812345678FF
    mov rax, 0x1234567812345678
    shrd rax, rdx, 8
    CHECK_EQ 10, rax, 0xFF12345678123456
    mov rax, 0
    mov rcx, 63
    bts rax, rcx
    CHECK_CC 12, nc
    CHECK_EQ 11, rax, 0x8000000000000000
    bt rax, rcx
    CHECK_CC 13, c
    btr rax, rcx
    CHECK_EQ 14, rax, 0
    btc rax, rcx
    CHECK_EQ 15, rax, 0x8000000000000000
    mov rax, 0x0000100000000000
    bsf rcx, rax
    CHECK_EQ 16, rcx, 44
    bsr rcx, rax
    CHECK_EQ 17, rcx, 44
    mov rax, 0xF0F0F0F0F0F0F0F0
    popcnt rcx, rax
    CHECK_EQ 18, rcx, 32
    mov rax, 0x0102030405060708
    bswap rax
    CHECK_EQ 19, rax, 0x0807060504030201
    mov rax, 0x00000000FFFFFFFF
    lzcnt rcx, rax
    CHECK_EQ 20, rcx, 32
    tzcnt rcx, rax
    CHECK_EQ 21, rcx, 0
    PASS
