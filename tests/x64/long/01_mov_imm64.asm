; EXPECT: xfail
; NATIVE: yes
; DESC: mov r64, imm64 (movabs), 32-bit writes zero-extend, 8/16-bit writes merge
%include "inc/long.inc"

test_main:
    mov rax, 0x1122334455667788
    CHECK_EQ 1, rax, 0x1122334455667788
    mov eax, 0x99AABBCC
    CHECK_EQ 2, rax, 0x99AABBCC                 ; upper half cleared
    mov rax, -1
    mov ax, 0x1234
    CHECK_EQ 3, rax, 0xFFFFFFFFFFFF1234         ; 16-bit write merges
    mov rax, -1
    mov al, 0x55
    CHECK_EQ 4, rax, 0xFFFFFFFFFFFFFF55
    mov rax, -1
    mov ah, 0x55
    CHECK_EQ 5, rax, 0xFFFFFFFFFFFF55FF
    mov rbx, 0x1111111111111111
    mov ebx, ebx
    CHECK_EQ 6, rbx, 0x11111111
    mov r9, 0xFEDCBA9876543210
    mov r9d, r9d
    CHECK_EQ 7, r9, 0x76543210
    PASS
