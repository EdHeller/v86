; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: mul / imul / div / idiv of every size, cmpxchg, xadd, fences, rdtsc and the sse moves and logic instructions the jit compiles
%include "inc/long.inc"

%define EXPECTED 0x10B91B22F6178454
%macro MIX 1
    xor r10, %1
    rol r10, 7
    add r10, r9
%endmacro

test_main:
    xor r10d, r10d
    mov r9d, 0x2545F491
    mov r12, 0x0123456789ABCDEF
    mov r13, 0xFEDCBA9876543210
    xor r11d, r11d
.loop:
    ; --- mul / imul / div / idiv
    mov rax, r12
    mov rdx, r13
    mul r13
    MIX rax
    MIX rdx
    pushfq
    pop rcx
    and ecx, 0x8C5          ; the flags mul defines (cf, of) and the ones left alone are not compared: cf of only
    and ecx, 0x801
    MIX rcx
    mov rax, r12
    imul r13
    MIX rax
    MIX rdx
    mov eax, r12d
    mov edx, 0
    mov ecx, 0x12345
    mul ecx
    MIX rax
    MIX rdx
    mov eax, r13d
    imul ecx
    MIX rax
    MIX rdx
    mov ax, r12w
    mov cx, 0x77
    mul cx
    MIX rax
    MIX rdx
    mov al, r12b
    mov cl, 0x33
    mul cl
    MIX rax
    mov al, r13b
    imul cl
    MIX rax
    lea rsi, [rel buf]
    mov rcx, 0x1000003
    mov [rsi], rcx
    mov rax, r12
    shr rax, 8
    xor edx, edx
    div qword [rsi]
    MIX rax
    MIX rdx
    mov rax, r13
    cqo
    mov rcx, 0x10001
    idiv rcx
    MIX rax
    MIX rdx
    mov eax, r12d
    xor edx, edx
    mov ecx, 0x3F1
    div ecx
    MIX rax
    MIX rdx
    mov eax, r13d
    cdq
    idiv ecx
    MIX rax
    MIX rdx
    mov ax, r12w
    xor dx, dx
    mov cx, 0x1F
    div cx
    MIX rax
    MIX rdx
    movzx eax, r12b
    mov cl, 9
    div cl
    MIX rax
    movsx ax, r13b
    mov cl, 7
    idiv cl
    MIX rax
    ; --- cmpxchg / xadd
    mov [rsi + 8], r12
    mov rax, r12
    mov rcx, r13
    cmpxchg [rsi + 8], rcx
    MIX rax
    MIX qword [rsi + 8]
    pushfq
    pop rcx
    and ecx, 0x8D5
    MIX rcx
    mov rax, r13
    mov rcx, 0x5555
    cmpxchg [rsi + 8], rcx
    MIX rax
    MIX qword [rsi + 8]
    mov eax, r12d
    mov edx, r12d
    mov ecx, 0x999
    cmpxchg edx, ecx
    MIX rax
    MIX rdx
    mov eax, 5
    mov edx, 7
    cmpxchg edx, ecx
    MIX rax
    MIX rdx
    mov [rsi + 16], r13
    mov rax, 3
    xadd [rsi + 16], rax
    MIX rax
    MIX qword [rsi + 16]
    mov rax, r12
    mov rdx, r13
    xadd rax, rdx
    MIX rax
    MIX rdx
    mov eax, 0x7FFFFFFF
    mov edx, 5
    xadd eax, edx
    MIX rax
    MIX rdx
    pushfq
    pop rcx
    and ecx, 0x8D5
    MIX rcx
    mov rax, r12
    xadd rax, rax
    MIX rax
    mov byte [rsi + 24], 0x41
    mov al, 0x41
    mov cl, 0x99
    cmpxchg [rsi + 24], cl
    MIX rax
    movzx edx, byte [rsi + 24]
    MIX rdx
    ; --- fences and rdtsc (only its order is checked)
    lfence
    mfence
    sfence
    rdtsc
    mov r14, rax
    shl rdx, 32
    or r14, rdx
    rdtsc
    shl rdx, 32
    or rax, rdx
    cmp rax, r14
    setae al
    movzx eax, al
    MIX rax
    ; --- sse moves and logic
    movq xmm0, r12
    movq xmm1, r13
    pxor xmm0, xmm1
    movq rax, xmm0
    MIX rax
    movd xmm2, r12d
    movd eax, xmm2
    MIX rax
    punpcklqdq xmm0, xmm1
    movups [rsi + 33], xmm0
    movups xmm3, [rsi + 33]
    movdqu [rsi + 64 + 5], xmm3
    movdqu xmm4, [rsi + 64 + 5]
    movdqa [rsi + 96], xmm4
    movdqa xmm5, [rsi + 96]
    movaps [rsi + 128], xmm5
    movaps xmm6, [rsi + 128]
    por xmm6, xmm1
    pand xmm6, xmm3
    pandn xmm6, xmm0
    xorps xmm6, xmm5
    andps xmm6, xmm4
    orps xmm6, xmm2
    andnps xmm6, xmm3
    movq rax, xmm6
    MIX rax
    movhlps xmm7, xmm6
    movups [rsi + 160], xmm6
    MIX qword [rsi + 160]
    MIX qword [rsi + 168]
    movq xmm7, [rsi + 160]
    movq [rsi + 176], xmm7
    MIX qword [rsi + 176]
    MIX qword [rsi + 184]
    movdqa xmm1, xmm6
    pxor xmm1, [rsi + 96]
    movdqa [rsi + 192], xmm1
    MIX qword [rsi + 192]
    MIX qword [rsi + 200]
    mov rax, r13
    mov [rsi + 208], rax
    movq xmm2, [rsi + 208]
    movd xmm3, [rsi + 208]
    movq rax, xmm2
    MIX rax
    movd eax, xmm3
    MIX rax
    movq rax, xmm3
    MIX rax
    ; next round
    rol r12, 13
    add r13, r12
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


    CHECK_EQ 1, r10, EXPECTED
    PASS

section .data
align 4096
buf: times 4096 db 0
hexbuf: times 20 db 0
