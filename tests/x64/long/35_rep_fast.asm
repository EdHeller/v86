; EXPECT: pass
; NATIVE: yes
; DESC: rep movs / stos of every size over page boundaries, overlapping forward copies, zero and large counts
%include "inc/long.inc"

%define EXPECTED 0xCFB997E51FD1FCEF
%macro MIX 1
    xor r10, %1
    rol r10, 7
    add r10, r9
%endmacro

%macro SUM 0
    ; fold the whole buffer into r10
    lea rsi, [rel buf]
    mov ecx, 3 * 4096 / 8
%%l:
    mov rax, [rsi]
    MIX rax
    add rsi, 8
    dec ecx
    jnz %%l
%endmacro

%macro FILL 0
    lea rdi, [rel buf]
    mov ecx, 3 * 4096 / 8
    mov rax, 0x0102030405060708
%%l:
    mov [rdi], rax
    mov rdx, 0x1111111111111111
    add rax, rdx
    add rdi, 8
    dec ecx
    jnz %%l
%endmacro

test_main:
    xor r10d, r10d
    mov r9d, 0x2545F491
    cld
    FILL
    ; rep movsb across the first page boundary, unaligned
    lea rsi, [rel buf + 100]
    lea rdi, [rel buf + 4096 + 3]
    mov ecx, 5000
    rep movsb
    SUM
    ; rep movsq / movsd / movsw
    lea rsi, [rel buf + 4090]
    lea rdi, [rel buf + 8192 + 5]
    mov ecx, 300
    rep movsq
    SUM
    lea rsi, [rel buf + 17]
    lea rdi, [rel buf + 4096 + 4000]
    mov ecx, 777
    rep movsd
    SUM
    lea rsi, [rel buf + 33]
    lea rdi, [rel buf + 2 * 4096 - 6]
    mov ecx, 1500
    rep movsw
    SUM
    ; overlapping forward copy: a repeating pattern
    lea rsi, [rel buf + 64]
    lea rdi, [rel buf + 64 + 3]
    mov ecx, 2000
    rep movsb
    SUM
    lea rsi, [rel buf + 8192 + 100]
    lea rdi, [rel buf + 8192 + 108]
    mov ecx, 100
    rep movsq
    SUM
    ; rep stos of each size, starting and ending in the middle of pages
    mov rax, 0xA1B2C3D4E5F60718
    lea rdi, [rel buf + 4000]
    mov ecx, 6000
    rep stosb
    SUM
    lea rdi, [rel buf + 123]
    mov ecx, 2500
    rep stosw
    SUM
    lea rdi, [rel buf + 4096 + 7]
    mov ecx, 1300
    rep stosd
    SUM
    lea rdi, [rel buf + 2 * 4096 - 24]
    mov ecx, 200
    rep stosq
    SUM
    ; count 0, count 1, and the registers after a long copy
    lea rsi, [rel buf]
    lea rdi, [rel buf + 4096]
    xor ecx, ecx
    rep movsb
    mov ecx, 1
    rep movsq
    lea rax, [rel buf]
    sub rsi, rax
    sub rdi, rax
    MIX rsi
    MIX rdi
    MIX rcx
    lea rsi, [rel buf + 10]
    lea rdi, [rel buf + 4096 + 20]
    mov ecx, 4097
    rep movsb
    sub rsi, 10
    sub rdi, 4096
    lea rax, [rel buf]
    sub rsi, rax
    sub rdi, rax
    MIX rsi
    MIX rdi
    MIX rcx

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


    CHECK_EQ 1, r10, EXPECTED
    PASS

section .data
align 4096
buf: times 3 * 4096 db 0
hexbuf: times 20 db 0
