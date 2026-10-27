; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: cmov, nop, bt/bts/btr/btc, string moves, xchg and cbw/cwde/cdqe/cqo in loops (kinds the jit64_disable_mask test needs covered)
%include "inc/long.inc"

%define EXPECTED 0xB4DDCD5D21143B73
%macro MIX 1
    xor r10, %1
    rol r10, 7
    add r10, r9
%endmacro

test_main:
    xor r10d, r10d
    mov r9d, 0x2545F491
    ; each kind gets a loop of its own, so that its block is short and certainly compiled
    xor r11d, r11d
.cmov:
    mov rax, r11
    mov rdx, 0x123456789ABCDEF0
    cmp rax, 20
    cmovb rdx, rax
    MIX rdx
    mov ecx, 5
    test r11d, 1
    cmovnz ecx, r11d
    MIX rcx
    inc r11d
    cmp r11d, 40
    jb .cmov

    xor r11d, r11d
.nop:
    nop
    nop
    mov rax, r11
    nop
    MIX rax
    inc r11d
    cmp r11d, 40
    jb .nop

    xor r11d, r11d
.bt:
    mov rax, 0x5555AAAA5555AAAA
    mov ecx, r11d
    and ecx, 63
    bt rax, rcx
    setc dl
    movzx edx, dl
    MIX rdx
    bts rax, rcx
    MIX rax
    btr rax, 3
    MIX rax
    btc rax, rcx
    MIX rax
    inc r11d
    cmp r11d, 40
    jb .bt

    cld
    xor r11d, r11d
.str:
    lea rsi, [rel src]
    lea rdi, [rel dst]
    mov ecx, 8
    rep movsq
    mov rax, r11
    lea rdi, [rel dst + 64]
    mov ecx, 4
    rep stosq
    mov rax, [rel dst + 8]
    MIX rax
    mov rax, [rel dst + 64 + 8]
    MIX rax
    lea rsi, [rel src + 8]
    lodsq
    MIX rax
    inc r11d
    cmp r11d, 40
    jb .str

    xor r11d, r11d
.xchg:
    mov rax, r11
    mov rbx, 0x0123456789ABCDEF
    xchg rax, rbx
    MIX rax
    MIX rbx
    xchg [rel scratch], rax
    MIX rax
    MIX qword [rel scratch]
    inc r11d
    cmp r11d, 40
    jb .xchg

    xor r11d, r11d
.sext:
    mov eax, r11d
    shl eax, 27
    cdqe
    MIX rax
    cqo
    MIX rdx
    cwde
    MIX rax
    cbw
    MIX rax
    inc r11d
    cmp r11d, 40
    jb .sext

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
