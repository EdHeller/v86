; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: tzcnt/lzcnt/popcnt, shld/shrd by an immediate, punpckhqdq, pcmpgtd, SHA-NI and rdtscp in loops (compiled, not left to the interpreter), values and flags
%include "inc/long.inc"

%define EXPECTED 0x07FA32603E716F61
%macro MIX 1
    xor r10, %1
    rol r10, 7
    add r10, r9
%endmacro
; the flags an instruction defines, into the mix
%macro MIXF 1
    pushfq
    pop rax
    and eax, %1
    MIX rax
%endmacro

test_main:
    xor r10d, r10d
    mov r9d, 0x2545F491

    xor r11d, r11d
.count:
    ; values with many shapes, zero among them (r11 = 0 and 33)
    mov rbx, r11
    imul rbx, rbx, 0x3F
    mov rcx, 0x0101010101010100
    imul rbx, rcx
    cmp r11d, 33
    cmove rbx, r11
    sub rbx, r11
    cmp r11d, 33
    jne .keep
    xor ebx, ebx
.keep:
    tzcnt rax, rbx
    MIX rax
    tzcnt rdx, rbx
    MIXF 0x41                    ; cf, zf
    lzcnt rax, rbx
    MIX rax
    lzcnt rdx, rbx
    MIXF 0x41
    popcnt rax, rbx
    MIX rax
    popcnt rdx, rbx
    MIXF 0x8D5                   ; popcnt defines all six
    tzcnt eax, ebx
    MIX rax
    lzcnt eax, ebx
    MIX rax
    popcnt eax, ebx
    MIX rax
    mov [rel scratch], rbx
    tzcnt rax, [rel scratch]
    MIX rax
    popcnt eax, dword [rel scratch + 4]
    MIX rax
    lzcnt ax, bx
    MIX rax
    inc r11d
    cmp r11d, 60
    jb .count

    xor r11d, r11d
.dshift:
    mov rax, 0x0123456789ABCDEF
    mov rdx, 0xFEDCBA9876543210
    add rax, r11
    mov rbx, rax
    shld rax, rdx, 13
    MIX rax
    mov rax, rbx
    shld rax, rdx, 13
    MIXF 0xC5                    ; cf, pf, zf, sf (of is defined for a count of 1 only)
    mov rax, rbx
    shrd rax, rdx, 1
    MIX rax
    mov rax, rbx
    shrd rax, rdx, 1
    MIXF 0x8C5
    mov rax, rbx
    shrd eax, edx, 7
    MIX rax
    mov rax, rbx
    shld eax, edx, 31
    MIX rax
    mov rax, rbx
    shld rax, rdx, 1
    MIXF 0x8C5
    mov [rel scratch], rbx
    shrd qword [rel scratch], rdx, 60
    MIX qword [rel scratch]
    mov [rel scratch], rbx
    shld dword [rel scratch], edx, 5
    MIX qword [rel scratch]
    inc r11d
    cmp r11d, 60
    jb .dshift

    xor r11d, r11d
.sse:
    movdqa xmm0, [rel va]
    movdqa xmm1, [rel vb]
    movd xmm2, r11d
    pshufd xmm2, xmm2, 0
    paddd xmm0, xmm2
    movdqa xmm3, xmm0
    punpckhqdq xmm0, xmm1
    movq rax, xmm0
    MIX rax
    psrldq xmm0, 8
    movq rax, xmm0
    MIX rax
    movdqa xmm0, xmm3
    punpckhqdq xmm0, [rel vb]
    movq rax, xmm0
    MIX rax
    movdqa xmm0, xmm3
    pcmpgtd xmm0, xmm1
    movq rax, xmm0
    MIX rax
    psrldq xmm0, 8
    movq rax, xmm0
    MIX rax
    movdqa xmm0, xmm3
    pcmpgtd xmm0, [rel va]
    movq rax, xmm0
    MIX rax
    inc r11d
    cmp r11d, 60
    jb .sse

    xor r11d, r11d
.sha:
    movdqa xmm1, [rel va]
    movdqa xmm2, [rel vb]
    movd xmm0, r11d
    pshufd xmm0, xmm0, 0x1B
    paddd xmm0, [rel vb]
    movdqa xmm3, xmm1
    sha256rnds2 xmm1, xmm2
    sha256msg1 xmm2, xmm1
    sha256msg2 xmm2, xmm3
    sha256rnds2 xmm2, [rel va]
    sha1nexte xmm3, xmm2
    sha1msg1 xmm3, xmm1
    sha1msg2 xmm3, [rel vb]
    sha1rnds4 xmm1, xmm3, 0
    sha1rnds4 xmm1, xmm2, 1
    sha1rnds4 xmm1, xmm3, 2
    sha1rnds4 xmm1, [rel va], 3
    pxor xmm1, xmm2
    pxor xmm1, xmm3
    movq rax, xmm1
    MIX rax
    psrldq xmm1, 8
    movq rax, xmm1
    MIX rax
    inc r11d
    cmp r11d, 60
    jb .sha

    ; rdtscp in a loop: the counter does not go back (ecx, IA32_TSC_AUX, is the machine's)
    xor r11d, r11d
    xor r8d, r8d
.tscp:
    rdtscp
    shl rdx, 32
    or rax, rdx
    cmp rax, r8
    jb .tscp_back
    mov r8, rax
    inc r11d
    cmp r11d, 60
    jb .tscp
    jmp .tscp_done
.tscp_back:
    MIX r8
.tscp_done:

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
va: dd 0x80000001, 0x7FFFFFFF, 0x00000010, 0xFFFFFFF0
vb: dd 0x00000005, 0x80000000, 0x00000010, 0x12345678
scratch: dq 0x1000
hexbuf: times 20 db 0
