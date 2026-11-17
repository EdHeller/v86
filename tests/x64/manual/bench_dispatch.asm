; DESC: benchmark: 10 M calls of a function on another page (every call and return leaves the block: the jit64 dispatcher); time worker.js on the flat binary
%include "inc/long.inc"

test_main:
    mov ecx, 10000000
    xor eax, eax
.loop:
    call far_function
    dec ecx
    jnz .loop
    PASS

align 4096
far_function:
    add rax, 3
    ret
