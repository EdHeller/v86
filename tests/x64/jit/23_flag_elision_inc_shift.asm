; EXPECT: pass
; DESC: 32-bit JIT: shifts and inc/dec whose flags are dead (only the result computed), next to a shift by cl whose CF adc reads (with counts of 0 that keep the CF of the add before it) and dec read only by jz; results from a model of x86, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov edi, 100000                 ; (enough to compile the loop)
    mov eax, 1
    mov ebx, 0x87654321
    mov edx, 0x13579bdf
    xor esi, esi
    mov ebp, 1000
.loop:
    shl eax, 3                      ; dead: add follows
    add eax, ebx                    ; its CF is read when cl & 31 is 0
    mov ecx, edx
    shr ebx, cl                     ; CF read by adc
    adc esi, 0
    sar edx, 5                      ; dead: xor follows
    xor edx, eax
    inc esi                         ; dead: test follows
    test esi, 7
    jnz .skip
    dec ebp                         ; read by jz only
    jz .wrap
    jmp .skip
.wrap:
    mov ebp, 1000
.skip:
    add ebx, 0x9e3779b9
    dec edi
    jnz .loop
    CHECK16 1, eax, 0xce36b2f3
    CHECK16 2, ebx, 0x9e3779bb
    CHECK16 3, edx, 0x31e2dc3d
    CHECK16 4, esi, 0x259d8
    CHECK16 5, ebp, 0x1f9
    PASS
