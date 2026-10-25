; EXPECT: xfail
; DESC: syscall/sysret through STAR/LSTAR/SFMASK, CPL0 -> CPL3 -> CPL0
%include "inc/long.inc"

test_main:
    mov ecx, 0xC0000081                         ; STAR: sysret base 0x20, syscall base 0x08
    xor eax, eax
    mov edx, 0x00200008
    wrmsr
    mov ecx, 0xC0000082                         ; LSTAR
    lea rax, [rel handler]
    mov rdx, rax
    shr rdx, 32
    wrmsr
    mov ecx, 0xC0000084                         ; SFMASK: clear IF and TF on entry
    mov eax, 0x300
    xor edx, edx
    wrmsr
    pushfq
    pop rax
    or rax, 0x200
    push rax
    popfq                                       ; IF=1 (no interrupt sources are active)
    xor r14d, r14d
    syscall
after_first:
    ; reached through sysretq at CPL3: cs=0x33, ss=0x2b
    mov ax, cs
    movzx r13d, ax
    mov ax, ss
    movzx r12d, ax
    syscall
handler:
    inc r14
    cmp r14, 2
    je .second
    ; first entry, from CPL0 syscall
    lea rax, [rel after_first]
    CHECK_EQ 1, rcx, rax                        ; rcx = rip after syscall
    mov r10, r11
    pushfq
    pop rax
    test eax, 0x200
    jz .if_cleared
    mov eax, 2
    xor r15d, r15d
    call report_fail
.if_cleared:
    test r10d, 0x200                            ; r11 keeps the old rflags with IF
    jnz .r11_ok
    mov eax, 3
    xor r15d, r15d
    call report_fail
.r11_ok:
    mov ax, cs
    movzx r9d, ax
    CHECK_EQ 4, r9, 0x08
    lea rcx, [rel after_first]                  ; return target
    or r11, 0x200
    o64 sysret                                  ; sysretq
.second:
    CHECK_EQ 5, r13, 0x33
    CHECK_EQ 6, r12, 0x2B
    mov ax, cs
    movzx r9d, ax
    CHECK_EQ 7, r9, 0x08
    mov ax, ss
    movzx r9d, ax
    CHECK_EQ 8, r9, 0x10
    PASS
