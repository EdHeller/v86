; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: imul, adc and sbb whose flags the next instructions overwrite (compiled inline, without the helper)
%include "inc/long.inc"

test_main:
    ; 64-bit: imul and adc with a register destination
    mov ecx, 5000
    mov rax, 1
    mov rbx, 3
    xor edx, edx
.loop64:
    imul rax, rbx
    add rax, 7
    adc rdx, rax
    imul rbx, rbx, 5
    or rbx, 1
    dec ecx
    jnz .loop64
    CHECK_EQ 1, rdx, 0x9e055e0006e734bc
    CHECK_EQ 2, rax, 0xb154a03743ad9b81

    ; the same with adc to memory
    push 0
    mov ecx, 5000
    mov rax, 1
    mov rbx, 3
.loopmem:
    imul rax, rbx
    add rax, 7
    adc qword [rsp], rax
    imul rbx, rbx, 5
    or rbx, 1
    dec ecx
    jnz .loopmem
    pop rdx
    CHECK_EQ 3, rdx, 0x9e055e0006e734bc

    ; 32-bit: imul and sbb (the upper halves cleared)
    mov ecx, 5000
    mov eax, 7
    mov ebx, 9
    xor esi, esi
.loop32:
    imul eax, ebx
    sub eax, 0x12345
    sbb esi, eax
    imul ebx, ebx, 3
    inc ebx
    dec ecx
    jnz .loop32
    CHECK_EQ 4, rsi, 0x5a8d8826
    CHECK_EQ 5, rax, 0x58598c8b
    PASS
