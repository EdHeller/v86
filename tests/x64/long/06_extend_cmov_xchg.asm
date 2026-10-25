; EXPECT: xfail
; NATIVE: yes
; DESC: movsxd, movzx/movsx to 64-bit, cdqe/cqo, cmovcc, setcc, xchg, xadd, cmpxchg
%include "inc/long.inc"

test_main:
    mov eax, 0x80000000
    movsxd rax, eax
    CHECK_EQ 1, rax, 0xFFFFFFFF80000000
    mov eax, 0x7FFFFFFF
    cdqe
    CHECK_EQ 2, rax, 0x7FFFFFFF
    mov eax, 0x80000000
    cdqe
    CHECK_EQ 3, rax, 0xFFFFFFFF80000000
    mov rbx, -1
    movzx rax, bl
    CHECK_EQ 4, rax, 0xFF
    movsx rax, bl
    CHECK_EQ 5, rax, 0xFFFFFFFFFFFFFFFF
    movzx rax, bx
    CHECK_EQ 6, rax, 0xFFFF
    movsx rax, bx
    CHECK_EQ 7, rax, 0xFFFFFFFFFFFFFFFF
    mov rax, 5
    mov rbx, 9
    cmp rax, rbx
    cmovb rax, rbx
    CHECK_EQ 8, rax, 9
    cmp rax, rbx
    sete cl
    movzx ecx, cl
    CHECK_EQ 9, rcx, 1
    xchg rax, rbx
    mov rax, 0x123456789ABCDEF0
    mov rbx, 0x0FEDCBA987654321
    xchg rax, rbx
    CHECK_EQ 10, rax, 0x0FEDCBA987654321
    CHECK_EQ 11, rbx, 0x123456789ABCDEF0
    mov rax, 0xFFFFFFFFFFFFFFFF
    mov rcx, 2
    xadd rax, rcx
    CHECK_EQ 12, rax, 1
    CHECK_EQ 13, rcx, 0xFFFFFFFFFFFFFFFF
    mov rax, 0x1111111111111111
    mov [rsp - 8], rax
    mov rdx, 0x2222222222222222
    cmpxchg [rsp - 8], rdx
    CHECK_CC 14, z
    CHECK_EQ 15, [rsp - 8], 0x2222222222222222
    mov rax, 5
    cmpxchg [rsp - 8], rdx                      ; mismatch -> rax = mem
    CHECK_CC 16, nz
    CHECK_EQ 17, rax, 0x2222222222222222
    PASS
