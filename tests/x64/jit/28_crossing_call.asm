; EXPECT: pass
; DESC: 32-bit JIT: a call rel32 that crosses into the next page (one displacement byte there), as a block of its own (a jump's target): it pushes its return address in the next page and goes to the callee; then the displacement byte in the next page is rewritten and the call goes to the other callee; results from a model, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov edi, 2                      ; two passes: the second after the call's displacement changed
.pass:
    mov ecx, 100000                 ; (enough to compile the loop)
    xor eax, eax
    call body
    cmp edi, 2
    jne .second
    CHECK16 1, eax, 100000
    inc byte [crossing_call + 2]    ; the displacement's second byte (in the next page): f1 -> f1 + 0x100 (f2)
    dec edi
    jmp .pass
.second:
    CHECK16 2, eax, 100000000
    PASS

align 4096
page0:
    times 4096 - 64 nop
body:
body_loop:
    jmp crossing_call               ; the call starts a block
    times (4096 - 2) - ($ - page0) nop
crossing_call:
    call f1                         ; E8 xx | xx xx xx: crosses into the next page, returns there
    dec ecx
    jnz body_loop
    ret

align 4096
f1:
    add eax, 1
    ret
    times 0x100 - ($ - f1) int3
f2:
    add eax, 1000
    ret
