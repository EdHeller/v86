; EXPECT: pass
; DESC: XSAVE header and XCR0 rules: init-state restore (XSTATE_BV=0), partial RFBM, XGETBV / XSETBV faults, alignment, reserved bits
%include "inc/real.inc"

test_main:
    SSE_ON
    ; without CR4.OSXSAVE xgetbv / xsetbv / xsave are #UD
    TRAP_BEGIN
        xor ecx, ecx
        xgetbv
    TRAP_END 1, 6
    TRAP_BEGIN
        xor ecx, ecx
        xor edx, edx
        mov eax, 3
        xsetbv
    TRAP_END 2, 6
    TRAP_BEGIN
        xor edx, edx
        mov eax, 3
        xsave [area]
    TRAP_END 3, 6
    mov eax, cr4
    or eax, 1 << 18
    mov cr4, eax
    xor ecx, ecx
    xor edx, edx
    mov eax, 3
    xsetbv
    ; xgetbv with a reserved index, xsetbv with reserved bits or without x87 => #GP
    TRAP_BEGIN
        mov ecx, 1
        xgetbv
    TRAP_END 4, 13
    TRAP_BEGIN
        xor ecx, ecx
        xor edx, edx
        mov eax, 0x80000003
        xsetbv
    TRAP_END 5, 13
    TRAP_BEGIN
        xor ecx, ecx
        mov edx, 1
        mov eax, 3
        xsetbv
    TRAP_END 6, 13
    xor ecx, ecx
    xgetbv
    CHECK16 7, eax, 3                           ; the failed xsetbv left xcr0 alone
    ; the save area has to be 64 byte aligned
    TRAP_BEGIN
        xor edx, edx
        mov eax, 3
        xsave [area + 8]
    TRAP_END 8, 13
    TRAP_BEGIN
        xor edx, edx
        mov eax, 3
        xrstor [area + 16]
    TRAP_END 9, 13
    ; save only the SSE component: the header says so, the x87 part of the area stays as it was
    mov dword [pattern], 0x01010101
    mov dword [pattern + 4], 0x02020202
    mov dword [pattern + 8], 0x03030303
    mov dword [pattern + 12], 0x04040404
    movups xmm4, [pattern]
    mov dword [area], 0x7777
    xor edx, edx
    mov eax, 2
    xsave [area]
    CHECK16 10, dword [area], 0x7777            ; x87 not requested: untouched
    mov eax, [area + 512]
    and eax, 3
    CHECK16 11, eax, 2                          ; only the SSE bit in XSTATE_BV
    mov eax, [area + 160 + 64]                  ; xmm4 at 160 + 16 * 4
    CHECK16 12, eax, 0x01010101
    ; restoring with XSTATE_BV = 0 puts the component in its initial state (xmm cleared)
    mov dword [area + 512], 0
    mov dword [area + 516], 0
    movups xmm4, [pattern]
    xor edx, edx
    mov eax, 2
    xrstor [area]
    CHECK_XMM 13, xmm4, 0, 0, 0, 0
    ; restoring only the x87 component leaves the xmm registers alone
    movups xmm4, [pattern]
    mov dword [area + 512], 3
    xor edx, edx
    mov eax, 1
    xrstor [area]
    CHECK_XMM 14, xmm4, 0x01010101, 0x02020202, 0x03030303, 0x04040404
    PASS
pattern: times 16 db 0
align 64
area: times 1024 db 0
