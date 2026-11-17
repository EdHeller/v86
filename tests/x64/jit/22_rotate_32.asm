; EXPECT: pass
; DESC: 32-bit JIT: rol and ror inline (immediate and cl counts, a cl count of 0 that leaves the flags alone, CF and OF read by adc, jo and pushfd, dead flags left out); results from a model of x86, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov edi, 100000                 ; (enough to compile the loop)
    mov eax, 0x12345678
    xor ebx, ebx
    mov edx, 0x9abcdef1
    xor esi, esi
    xor ebp, ebp
.loop:
    rol eax, 7
    adc ebx, 0                      ; CF of rol
    ror eax, 3
    jno .n1                         ; OF of ror
    inc esi
.n1:
    add eax, ebx
    rol eax, 13                     ; flags dead: cmp follows
    mov ecx, eax
    cmp esi, ebx                    ; the flags seen when cl & 31 is 0
    rol edx, cl
    pushfd
    pop ecx
    and ecx, 0x801                  ; CF and OF
    add ebp, ecx
    mov ecx, ebx
    cmp edx, eax
    ror edx, cl
    pushfd
    pop ecx
    and ecx, 0x801
    add ebp, ecx
    xor edx, eax
    dec edi
    jnz .loop
    CHECK16 1, eax, 0xcac5d416
    CHECK16 2, ebx, 0xc366
    CHECK16 3, edx, 0x260fdc15
    CHECK16 4, esi, 0xc40c
    CHECK16 5, ebp, 0xbf5322e
    PASS
