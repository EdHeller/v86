; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: byte copy that skips a transparent value (the loop TempleOS spends most of its time in)
%include "inc/long.inc"

%define N 3000
%define EXPECTED 296632

test_main:
    ; source: byte i is 0xFF when i % 3 == 0, otherwise (i & 0x7F) | 1
    lea rdi, [rel src]
    xor ecx, ecx
.fill:
    mov eax, ecx
    and al, 0x7F
    or al, 1
    xor edx, edx
    mov r8d, ecx
    ; i % 3 without div: subtract 3 repeatedly is too slow, use a counter instead
    movzx r9d, byte [rel phase]
    cmp r9d, 0
    jne .store
    mov al, 0xFF
.store:
    mov [rdi + rcx], al
    inc r9d
    cmp r9d, 3
    jb .keep
    xor r9d, r9d
.keep:
    mov [rel phase], r9b
    inc ecx
    cmp ecx, N
    jb .fill

    ; destination is pre-filled with 0xAA
    lea rdi, [rel dst]
    mov ecx, N
    mov al, 0xAA
    rep stosb

    ; the blit
    lea r11, [rel src]
    lea rsi, [rel dst]
    xor r13d, r13d
    mov r10d, N
.blit:
    cmp r13, r10
    jge .end
    movzx r12, byte [r11]
    inc r11
    cmp r12, 0xff
    je .skip
    mov [rsi], r12b
    inc rsi
    jmp .next
.skip:
    inc rsi
.next:
    inc r13
    jmp .blit
.end:
    ; checksum of the destination: transparent positions keep 0xAA
    lea rsi, [rel dst]
    xor eax, eax
    xor ecx, ecx
.sum:
    movzx edx, byte [rsi + rcx]
    add rax, rdx
    inc ecx
    cmp ecx, N
    jb .sum
    CHECK_EQ 1, rax, EXPECTED
    CHECK_EQ 2, r13, N
    PASS

section .data
phase: db 0
align 16
src: times N db 0
dst: times N db 0
