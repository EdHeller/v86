; EXPECT: xfail
; DESC: XSETBV/XGETBV and XSAVE/XRSTOR/XSAVEOPT with x87+SSE state (CR4.OSXSAVE)
%include "inc/real.inc"

test_main:
    SSE_ON
    mov eax, 1
    cpuid
    test ecx, 1 << 26                           ; CPUID.1:ECX.XSAVE
    jnz .has_xsave
    FAIL16 1
.has_xsave:
    mov eax, cr4
    or eax, 1 << 18                             ; OSXSAVE
    mov cr4, eax
    xor ecx, ecx
    xgetbv
    test eax, 1                                 ; XCR0.x87 is always set
    jnz .x87
    FAIL16 2
.x87:
    xor ecx, ecx
    mov eax, 3                                  ; x87 | SSE
    xor edx, edx
    xsetbv
    xor ecx, ecx
    xgetbv
    CHECK16 3, eax, 3
    mov dword [pattern], 0x11111111
    mov dword [pattern + 4], 0x22222222
    mov dword [pattern + 8], 0x33333333
    mov dword [pattern + 12], 0x44444444
    movups xmm3, [pattern]
    mov eax, 3
    xor edx, edx
    xsave [area]
    pxor xmm3, xmm3
    CHECK_XMM 4, xmm3, 0, 0, 0, 0
    mov eax, 3
    xor edx, edx
    xrstor [area]
    CHECK_XMM 5, xmm3, 0x11111111, 0x22222222, 0x33333333, 0x44444444
    ; xsave header: XSTATE_BV (offset 512) records which components are in use
    mov eax, [area + 512]
    and eax, 2
    CHECK16 6, eax, 2                           ; the SSE component is in use (non-init)
    ; xsaveopt must be accepted as well
    mov eax, 3
    xor edx, edx
    xsaveopt [area]
    ; xsetbv without the x87 bit => #GP
    TRAP_BEGIN
        xor ecx, ecx
        mov eax, 2
        xor edx, edx
        xsetbv
    TRAP_END 7, 13
    PASS
pattern: times 16 db 0
align 64
area: times 1024 db 0
