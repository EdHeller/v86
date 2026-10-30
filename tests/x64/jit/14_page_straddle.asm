; EXPECT: pass
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: loops whose instructions cross a 4 KiB page boundary (code run from the interpreter at the boundary, the next page compiled from an entry in the middle)
%include "inc/long.inc"
%define EXPECTED 0x6678e614

test_main:
    xor r10d, r10d
    mov r11d, 300
    mov r9d, 0x2545F491
    jmp .pad
; the instruction below crosses the page at 0x9000 (the image is loaded at 0x7C00)
times (0x1400 - ($ - $$) - 4) db 0x90
.pad:
.loop:
    mov rax, 0x1122334455667788        ; 10 bytes, 4 of them before the boundary
    add rax, r11
    xor r10, rax
    rol r10, 7
    add r10, r9
    lea rcx, [r10 + r11 * 4 + 0x12345]
    xor r10, rcx
    mov edx, 0x99887766
    bswap edx
    add r10, rdx
    dec r11d
    jnz .loop
    CHECK_EQ 1, r10, EXPECTED
    PASS
