; EXPECT: pass
; DESC: 32-bit JIT: the flags of a cmp/test read only by the conditional jump after it (which computes them from registers), dead in both successors; next to a cmp whose flags adc reads; interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov edi, 100000
    mov eax, 7
    mov ebx, 0x12345
    xor edx, edx
    xor esi, esi
    xor ebp, ebp
.loop:
    cmp eax, ebx                    ; read only by jb: both successors overwrite the flags first
    jb .lo
    sub eax, ebx
    jmp .join
.lo:
    add eax, 0x9E3779B9
    inc esi
.join:
    test eax, eax                   ; read only by jnz
    jnz .nz
    inc ebp
.nz:
    add ebx, 0x1111
    cmp ebx, 0x10000000             ; read by adc
    adc edx, 0
    dec edi
    jnz .loop
    CHECK16 1, eax, 0x4e0f726e
    CHECK16 2, ebx, 0x1a0bb3e5
    CHECK16 3, edx, 0xefef
    CHECK16 4, esi, 0x1d04
    CHECK16 5, ebp, 0
    PASS
