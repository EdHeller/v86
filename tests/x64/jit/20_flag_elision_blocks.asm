; EXPECT: pass
; DESC: 32-bit JIT: flags written at the end of a basic block, dead when the next block (a jump target) overwrites them first, kept when it reads them first (adc); interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov edi, 100000
    mov eax, 1
    mov ebx, 0x9E3779B9
    xor edx, edx
    xor esi, esi
    test edi, edi
    jz .t1                          ; (never taken: makes .t1 and .t2 block starts)
    test edi, edi
    jz .t2
.loop:
    add eax, ebx                    ; dead: the block of .t1 starts with cmp
.t1:
    cmp eax, 0x80000000
    jae .high
    inc esi
.high:
    sub ebx, 0x30000000             ; read: the block of .t2 starts with adc
.t2:
    adc edx, 0
    mov ecx, eax
    shr ecx, 3
    add ebx, ecx
    dec edi
    jnz .loop
    CHECK16 1, eax, 0x1546c6a5
    CHECK16 2, ebx, 0x6f548e59
    CHECK16 3, edx, 0x4916
    CHECK16 4, esi, 0xc3d7
    PASS
