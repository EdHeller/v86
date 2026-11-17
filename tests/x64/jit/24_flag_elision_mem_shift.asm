; EXPECT: pass
; DESC: 32-bit JIT: flags made dead by arithmetic with a memory operand (cmp reg, [mem]; add [mem], reg) and by shl by an immediate, next to the flags those set being read (sete, adc, setz); results from a model of x86, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov edi, 100000                 ; (enough to compile the loop)
    mov eax, 3
    mov ebx, 0x10001
    mov edx, 7
    xor esi, esi
    xor ebp, ebp
    mov dword [scratch], 5
    mov dword [scratch + 4], 0
.loop:
    add eax, ebx                    ; dead: cmp with memory follows
    cmp eax, [scratch]
    sete cl
    movzx ecx, cl
    add esi, ecx
    sub edx, eax                    ; dead: add to memory follows
    add [scratch + 4], edx
    adc esi, 0                      ; CF of the add to memory
    xor ebx, esi                    ; dead: shl follows
    shl ebx, 1
    setz cl                         ; ZF of shl
    movzx ecx, cl
    add esi, ecx
    add ebx, 0x9e3779b9
    add [scratch], ebp
    inc ebp
    dec edi
    jnz .loop
    CHECK16 1, eax, 0xee4a0791
    CHECK16 2, ebx, 0x55a40e3d
    CHECK16 3, edx, 0x4c0d659
    CHECK16 4, esi, 0xc393
    mov eax, [scratch]
    CHECK16 5, eax, 0x2a052eb5
    mov eax, [scratch + 4]
    CHECK16 6, eax, 0x2f840b18
    PASS

align 4096                          ; (its own page: stores to it leave the compiled loop alone)
scratch: dd 0, 0
