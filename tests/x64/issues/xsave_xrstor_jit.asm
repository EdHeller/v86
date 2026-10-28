; EXPECT: pass
; DESC: xsave / xrstor / xsaveopt in code long enough to be compiled (a 32-bit Linux kernel switches fpu state with xrstor; it was an unimplemented stub in compiled code)
%include "inc/real.inc"

test_main:
    SSE_ON
    mov eax, cr4
    or eax, 1 << 18                             ; OSXSAVE
    mov cr4, eax
    xor ecx, ecx
    mov eax, 3                                  ; x87 | SSE
    xor edx, edx
    xsetbv
    xor esi, esi                                ; iteration
.loop:
    mov [pattern], esi
    mov [pattern + 4], esi
    mov [pattern + 8], esi
    mov [pattern + 12], esi
    movups xmm5, [pattern]
    mov eax, 3
    xor edx, edx
    xsave [area]
    pxor xmm5, xmm5
    mov eax, 3
    xor edx, edx
    xrstor [area]
    movups [check], xmm5
    mov eax, [check]
    cmp eax, esi
    jne .bad
    mov eax, 3
    xor edx, edx
    xsaveopt [area]
    inc esi
    cmp esi, 40000
    jb .loop
    CHECK16 1, dword [EXC_VEC], -1
    PASS
.bad:
    FAIL16 2
pattern: times 16 db 0
check: times 16 db 0
align 64
area: times 1024 db 0
