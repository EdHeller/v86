; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: bt family, string instructions, neg / not / imul / xchg / cdq, indirect call and jump, push r/m (expected value derived natively)
%include "inc/long.inc"

%define EXPECTED 0x0e2a0ed322b81e70
%macro MIX 1
    xor r10, %1
    rol r10, 5
    add r10, 0x1E3779B1
%endmacro

test_main:
    xor r10d, r10d
    lea rbx, [rel src]
    lea rdi, [rel dst]
    xor ecx, ecx
    mov rax, 0x1122334455667788
    mov rdx, 0x0101010101010101
.fill:
    mov [rbx + rcx*8], rax
    add rax, rdx
    imul rax, rax, 3
    inc ecx
    cmp ecx, 64
    jb .fill

    xor r11d, r11d
.main:
    ; bt / bts / btr / btc on registers and memory, immediate and register bit offsets
    mov rax, [rbx + r11*8]
    mov ecx, r11d
    imul ecx, ecx, 37
    bt rax, rcx
    setc dl
    movzx edx, dl
    MIX rdx
    bts rax, rcx
    MIX rax
    btr rax, 5
    MIX rax
    btc eax, 31
    MIX rax
    bt qword [rbx + 8], rcx         ; the bit offset may address beyond the operand
    setc dl
    movzx edx, dl
    MIX rdx
    mov rsi, rcx
    neg rsi
    bts qword [rdi + 64], rsi       ; negative offset
    mov rax, [rdi + 56]
    MIX rax
    mov rax, [rdi + 64]
    MIX rax
    btr dword [rdi + 64], 3
    btc word [rdi + 66], 9
    bt dword [rbx + 4], 17
    jnc .nc
    MIX r11
.nc:
    mov rax, [rdi + 64]
    MIX rax

    ; single string instructions
    lea rsi, [rbx + r11*8]
    lodsq
    MIX rax
    lodsd
    MIX rax
    lodsb
    MIX rax
    lodsw
    MIX rax
    sub rsi, rbx
    MIX rsi
    lea r8, [rdi + 128]
    mov rdi, r8
    mov rax, 0x0F0E0D0C0B0A0908
    stosq
    stosd
    stosw
    stosb
    lea rsi, [rbx + 8]
    movsq
    movsd
    movsw
    movsb
    sub rdi, r8
    MIX rdi
    lea rdi, [rel dst]
    mov rax, [rdi + 128]
    MIX rax
    mov rax, [rdi + 136]
    MIX rax
    mov rax, [rdi + 144]
    MIX rax
    std
    lea rsi, [rbx + 40]
    lea rdi, [rel dst + 200]
    movsq
    movsd
    lodsb
    stosb
    cld
    lea r8, [rel dst + 200]
    sub r8, rdi
    MIX r8
    mov rax, [rel dst + 184]
    MIX rax
    ; rep prefixed
    lea rdi, [rel dst + 256]
    mov ecx, 9
    mov eax, 0x5A5A5A5A
    rep stosd
    lea rsi, [rbx + r11]
    lea rdi, [rel dst + 320]
    mov ecx, 5
    rep movsq
    mov rax, [rel dst + 256]
    MIX rax
    mov rax, [rel dst + 336]
    MIX rax
    lea rdi, [rel dst + 320]
    mov rax, [rbx + 8]
    mov ecx, 8
    repne scasq
    MIX rcx
    lea rdi, [rel dst]
    lea rsi, [rel dst + 320]
    mov ecx, 5
    repe cmpsb
    MIX rcx
    lea rdi, [rel dst]

    ; neg / not / test with an immediate / xchg
    mov rax, r11
    neg rax
    MIX rax
    jns .pos
    MIX r11
.pos:
    not rax
    MIX rax
    mov eax, 0x80000000
    neg eax
    seto dl
    movzx edx, dl
    MIX rdx
    neg byte [rdi + 0x180]
    mov rax, [rdi + 0x180]
    MIX rax
    not dword [rdi + 0x184]
    mov rax, [rdi + 0x184]
    MIX rax
    test dword [rdi], 0x00FF
    setz dl
    movzx edx, dl
    MIX rdx
    mov rax, r11
    mov rcx, 0x123456789
    xchg rax, rcx
    MIX rax
    MIX rcx
    xchg [rdi + 0x190], rcx
    MIX rcx
    mov rax, [rdi + 0x190]
    MIX rax
    xchg ah, al

    ; sign extension and multiplication
    mov rax, 0x80
    cbw
    MIX rax
    mov eax, 0x8000
    cwde
    MIX rax
    mov eax, 0x80000000
    cdqe
    MIX rax
    cdq
    MIX rdx
    mov rax, 0x8000000000000000
    cqo
    MIX rdx
    mov rax, 0x7FFFFFFFFFFFFFFF
    imul rax, rax
    seto dl
    movzx edx, dl
    MIX rax
    MIX rdx
    mov ecx, r11d
    imul ecx, [rbx + r11*8 + 4]
    MIX rcx
    imul r12w, [rbx + 6], 1234
    MIX r12
    imul r13, [rbx + r11*8], -77
    MIX r13

    ; push r/m, indirect call and jump
    push qword [rbx + r11*8]
    push r11
    pop rax
    pop rcx
    MIX rax
    MIX rcx
    lea rax, [rel callee]
    call rax
    MIX rax
    mov [rel fptr], rax
    lea rax, [rel callee2]
    mov [rel fptr], rax
    call [rel fptr]
    MIX rax
    lea rax, [rel .after_jmp]
    jmp rax
    MIX r11                         ; skipped
.after_jmp:
    jmp [rel jptr]
    MIX r11                         ; skipped
.jt:
    inc r11d
    cmp r11d, 40
    jb .main

    CHECK_EQ 1, r10, EXPECTED
    PASS

callee:
    mov rax, 0x77
    ret
callee2:
    lea rax, [r11 + 0x100]
    ret

section .data
align 16
src: times 80 dq 0
dst: times 80 dq 0
fptr: dq 0
jptr: dq test_main.jt
