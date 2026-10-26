; EXPECT: pass
; V86: {"jit64_threshold": 5, "sync_jit": true}
; DESC: jit64: a loop that rewrites an instruction of its own block in every iteration
%include "inc/long.inc"

test_main:
    xor eax, eax
    xor edx, edx
    mov ecx, 100
.l:
    mov byte [rel .patch + 1], cl       ; the immediate of the next instruction
.patch:
    mov dl, 0x11
    add eax, edx
    dec ecx
    jnz .l
    CHECK_EQ 1, rax, 5050

    ; code written once and executed many times from another page-aligned buffer
    lea rdi, [rel buf]
    mov esi, 3
.gen:
    ; mov eax, esi ; add eax, imm8 (esi) ; ret
    mov byte [rdi], 0x89
    mov byte [rdi + 1], 0xF0
    mov byte [rdi + 2], 0x83
    mov byte [rdi + 3], 0xC0
    mov [rdi + 4], sil                  ; add eax, <esi>
    mov byte [rdi + 5], 0xC3
    mov r12d, 20
.run:
    call rdi
    dec r12d
    jnz .run
    add esi, 1
    cmp esi, 40
    jb .gen
    CHECK_EQ 2, rax, 78                 ; last generated code: eax = esi + esi with esi = 39
    PASS

section .data
align 4096
buf: times 64 db 0xCC
