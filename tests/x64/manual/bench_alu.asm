; DESC: benchmark: 30 M iterations of imul, adc with their flags not needed (time worker.js on the flat binary)
%include "inc/long.inc"

test_main:
    mov ecx, 30000000
    mov rax, 1
    mov rbx, 3
    xor edx, edx
.loop:
    imul rax, rbx            ; flags dead: overwritten by the add
    add rax, 7
    adc rdx, rax             ; flags dead: overwritten by the dec
    imul rbx, rbx, 5
    or rbx, 1
    dec ecx
    jnz .loop
    PASS
