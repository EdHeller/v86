; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: bswap of 32 and 64 bit registers (low and high, old and new, with and without REX.B) in a loop
%include "inc/long.inc"

%define EXPECTED 0xD3A1974AEC3891A7
%macro MIX 1
    xor r10, %1
    rol r10, 7
    add r10, r9
%endmacro

test_main:
    xor r10d, r10d
    mov r9d, 0x2545F491
    mov rax, 0x0123456789ABCDEF
    mov rbx, 0xFEDCBA9876543210
    xor r11d, r11d
.loop:
    mov rcx, rax
    bswap ecx
    MIX rcx
    mov rdx, rbx
    bswap rdx
    MIX rdx
    mov r8, rax
    bswap r8
    MIX r8
    mov r12, rbx
    bswap r12d
    MIX r12
    mov r13, 0xFFFFFFFF00000000
    or r13, rax
    bswap r13d
    MIX r13
    bswap rax
    add rax, r11
    rol rbx, 13
    inc r11d
    cmp r11d, 40
    jb .loop

%ifdef DUMP
    ; print r10 as 16 hex digits
    lea rdi, [rel hexbuf]
    mov rax, r10
    mov ecx, 16
.hex:
    rol rax, 4
    mov edx, eax
    and edx, 15
    add dl, '0'
    cmp dl, '9'
    jbe .digit
    add dl, 7
.digit:
    mov [rdi], dl
    inc rdi
    dec ecx
    jnz .hex
    mov byte [rdi], 10
    mov eax, 1
    mov edi, 1
    lea rsi, [rel hexbuf]
    mov edx, 17
    syscall
    mov eax, 60
    xor edi, edi
    syscall
%endif
    CHECK_EQ 1, r10, EXPECTED
    PASS

section .data
align 4096
align 16
src: dq 1, 2, 3, 4, 5, 6, 7, 8, 0x1111111111111111, 0x2222222222222222
dst: times 20 dq 0
scratch: dq 0x1000
hexbuf: times 20 db 0
