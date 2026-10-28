; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: instructions the jit hands to the interpreter from inside a block (pushf/popf, cmpxchg, xadd, bsf/bsr, pause, cld/std/stc/clc/cmc, mul/div) between compiled ones
%include "inc/long.inc"

%define EXPECTED 0x4F556172E49B50A6
%macro MIX 1
    xor r10, %1
    rol r10, 7
    add r10, r9
%endmacro

test_main:
    xor r10d, r10d
    mov r9d, 0x2545F491
    xor r11d, r11d
.iter:
    mov rax, r11
    imul rax, rax, 0x9E3779B
    add rax, 0x1234
    mov rbx, rax
    shr rbx, 3
    or rbx, 1
    ; flags through pushf / popf
    add rax, rbx
    pushfq
    pop rcx
    and ecx, 0x8D5
    MIX rcx
    mov rcx, 0x8D5
    push rcx
    popfq
    pushfq
    pop rcx
    and ecx, 0x8D5
    MIX rcx
    ; carry flag instructions
    stc
    mov rdx, 5
    adc rdx, 0
    MIX rdx
    clc
    cmc
    adc rdx, rdx
    MIX rdx
    cld
    pause
    ; cmpxchg / xadd on memory
    mov [rel cell], rbx
    mov rax, rbx
    mov rcx, r11
    cmpxchg [rel cell], rcx
    MIX qword [rel cell]
    mov rax, rbx
    xadd [rel cell], rax
    MIX rax
    MIX qword [rel cell]
    ; bsf / bsr
    lea rax, [rbx + 0x100]
    bsf rcx, rax
    MIX rcx
    bsr rcx, rax
    MIX rcx
    ; mul / div (rdx:rax)
    mov rax, rbx
    mov rcx, 1000003
    xor edx, edx
    mul rcx
    MIX rdx
    MIX rax
    xor edx, edx
    div rcx
    MIX rax
    MIX rdx
    inc r11d
    cmp r11d, 300
    jb .iter

%ifdef DUMP
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
cell: dq 0
hexbuf: times 20 db 0
