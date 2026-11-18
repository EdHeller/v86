; EXPECT: pass
; DESC: 32-bit JIT: an instruction that crosses into the next page compiled at the end of its block (add eax, imm32 with two immediate bytes in the next page), then those bytes rewritten: the compiled code sees the change and the new immediate is used; results from a model, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov edi, 2                      ; two passes: the second after the immediate's high bytes changed
.pass:
    mov ecx, 100000                 ; (enough to compile the loop)
    xor eax, eax
    xor ebx, ebx
    xor edx, edx
    call body
    cmp edi, 2
    jne .second
    CHECK16 1, eax, 0xb1d9fa80
    CHECK16 2, ebx, 0x13266719
    CHECK16 3, edx, 0x3e56da80
    ; the immediate's two bytes in the next page: 0x0102 -> 0x0506
    mov byte [crossing + 3], 0x06
    mov byte [crossing + 4], 0x05
    dec edi
    jmp .pass
.second:
    CHECK16 4, eax, 0x4c59fa80
    CHECK16 5, ebx, 0x7378d4c0
    CHECK16 6, edx, 0xff06da80
    PASS

align 4096
page0:
    times 4096 - 64 nop
body:
body_loop:
    add ebx, eax
    rol ebx, 3
    times (4096 - 3) - ($ - page0) nop
crossing:
    add eax, 0x01020304             ; 05 04 03 | 02 01: crosses into the next page
    xor edx, eax
    dec ecx
    jnz body_loop
    ret
