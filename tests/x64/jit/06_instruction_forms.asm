; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: addressing modes, operand sizes, register forms and read-modify-write instructions in loops (expected value derived natively)
%include "inc/long.inc"

; r10 accumulates a checksum of everything computed below
%define EXPECTED 0x377b7ee4f9abdbfe
%macro MIX 1
    xor r10, %1
    rol r10, 5
    add r10, 0x1E3779B1
%endmacro

test_main:
    xor r10d, r10d
    lea rbx, [rel src]
    lea rdi, [rel dst]
    ; fill the source with a pattern: src[i] = i * 0x0101010101010101 + 0x1122334455667788
    xor ecx, ecx
    mov rax, 0x1122334455667788
    mov rdx, 0x0101010101010101
.fill:
    mov [rbx + rcx*8], rax
    add rax, rdx
    inc ecx
    cmp ecx, 64
    jb .fill

    xor r11d, r11d                  ; loop counter
.main:
    ; loads of every size, zero and sign extension
    mov al, [rbx + r11]
    MIX rax
    movzx eax, byte [rbx + r11 + 1]
    MIX rax
    movsx rax, byte [rbx + r11 + 2]
    MIX rax
    movzx eax, word [rbx + r11*2]
    MIX rax
    movsx rax, word [rbx + r11*2 + 2]
    MIX rax
    movsxd rax, dword [rbx + r11*4]
    MIX rax
    mov eax, [rbx + r11*4 + 8]
    MIX rax
    mov rax, [rbx + r11*8]
    MIX rax
    mov rax, -1
    mov ax, [rbx + r11*8 + 16]      ; 16 bit load keeps the upper bits
    MIX rax
    mov rax, -1
    mov al, [rbx + r11*8 + 17]      ; 8 bit load keeps the upper bits
    MIX rax
    mov rax, [rbx + r11*8 + 0x100]
    MIX rax
    mov rax, [rbx + 0x20]
    MIX rax
    mov rax, [rel src + 0x28]
    MIX rax

    ; lea
    lea rax, [rbx + r11*4 + 0x10]
    sub rax, rbx
    MIX rax
    lea eax, [r11*8 + 5]
    MIX rax
    lea r12, [r11 + r11*2]
    MIX r12
    lea rax, [rbx + r11]
    sub rax, rbx
    MIX rax

    ; stores
    mov rax, 0xA1B2C3D4E5F60718
    mov [rdi + r11*8], rax
    mov dword [rdi + r11*8 + 4], 0x55667788
    mov word [rdi + r11*8 + 2], 0x1234
    mov byte [rdi + r11*8 + 1], 0x99
    mov rax, [rdi + r11*8]
    MIX rax
    mov qword [rdi + 0x100], -2
    mov rax, [rdi + 0x100]
    MIX rax

    ; read-modify-write
    mov rax, r11
    add [rdi + r11*8], rax
    sub [rdi + r11*8], rax
    xor [rdi + r11*8], rax
    or dword [rdi + r11*8 + 4], 0x10
    and qword [rdi + r11*8], -2
    mov rax, [rdi + r11*8]
    MIX rax
    inc qword [rdi + 0x108]
    dec dword [rdi + 0x110]
    inc byte [rdi + 0x118]
    mov rax, [rdi + 0x108]
    MIX rax
    mov rax, [rdi + 0x110]
    MIX rax
    mov rax, [rdi + 0x118]
    MIX rax
    shl qword [rdi + 0x108], 3
    shr dword [rdi + 0x110], 1
    sar word [rdi + 0x118], 2
    rol dword [rdi + 0x120], 5
    ror byte [rdi + 0x128], 3
    mov rax, [rdi + 0x108]
    MIX rax
    mov rax, [rdi + 0x110]
    MIX rax
    mov rax, [rdi + 0x118]
    MIX rax
    mov rax, [rdi + 0x120]
    MIX rax
    mov rax, [rdi + 0x128]
    MIX rax

    ; rip-relative read-modify-write
    mov rax, [rel scratch]
    add rax, r11
    mov [rel scratch], rax
    add qword [rel scratch], 3
    mov rax, [rel scratch]
    MIX rax

    ; register forms: 8 bit registers including the legacy high bytes and the rex low bytes
    xor esi, esi
    xor edi, edi
    xor ebp, ebp
    xor r8d, r8d
    mov rax, 0x1111111111111111
    mov rcx, 0x2222222222222222
    mov ah, cl
    mov al, ch
    mov sil, al
    mov dil, cl
    mov r8b, sil
    mov bpl, dil
    mov r9d, 0x80000000
    movsxd r9, r9d
    MIX rax
    MIX rsi
    MIX rdi
    MIX rbp
    MIX r8
    MIX r9
    xor esi, esi
    xor ebp, ebp
    lea rdi, [rel dst]

    ; sizes of arithmetic
    mov rax, r11
    imul rax, rax, 0x1234
    add eax, 0x7FFFFFFF
    adc rax, 0
    mov r12, rax
    sub r12w, 0x7FFF
    sbb r12b, 1
    MIX rax
    MIX r12
    mov rcx, r11
    neg rcx
    not rcx
    sar rcx, 2
    shr ecx, 1
    MIX rcx

    ; setcc / cmov with memory operands
    cmp r11, 20
    setb [rdi + 0x130]
    setg byte [rdi + 0x131]
    mov rax, [rdi + 0x130]
    MIX rax
    mov rax, 0x123456789
    mov rdx, 0x7766554433221100
    cmp r11, 30
    cmovb rax, rdx
    cmovae rdx, [rbx + r11*8]
    MIX rax
    MIX rdx

    ; push / pop of all sizes
    push r11
    push 0x12345678
    push word 0x1357
    mov rax, 0xFFFFFFFFFFFFFFFF
    pop ax
    pop r13
    pop r14
    MIX rax
    MIX r13
    MIX r14

    ; 32 bit addressing
    lea esi, [rel src]
    mov eax, [esi + r11d*4]
    MIX rax
    mov rcx, r11
    mov eax, [esi + ecx*8 + 8]
    MIX rax

    inc r11d
    cmp r11d, 40
    jb .main

    CHECK_EQ 1, r10, EXPECTED
    PASS

section .data
align 16
src: times 80 dq 0
dst: times 80 dq 0
scratch: dq 0x1000
