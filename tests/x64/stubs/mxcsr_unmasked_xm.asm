; EXPECT: pass
; QEMU: xfail  (qemu's TCG never delivers #XM / SIMD FP exceptions; expected values follow the SDM)
; DESC: unmasked SIMD FP exceptions raise #XM (int 19), or #UD when CR4.OSXMMEXCPT is clear; reserved MXCSR bits #GP
%include "inc/real.inc"

test_main:
    SSE_ON
    ; unmask divide-by-zero: #XM
    mov dword [mxcsr_tmp], 0x1F80 & ~(1 << 9)
    ldmxcsr [mxcsr_tmp]
    TRAP_BEGIN
        movss xmm0, [one]
        divss xmm0, [zero]
    TRAP_END 1, 19
    ; unmask invalid operation
    mov dword [mxcsr_tmp], 0x1F80 & ~(1 << 7)
    ldmxcsr [mxcsr_tmp]
    TRAP_BEGIN
        movss xmm0, [zero]
        divss xmm0, [zero]
    TRAP_END 2, 19
    ; with CR4.OSXMMEXCPT clear an unmasked exception becomes #UD
    mov eax, cr4
    and eax, ~(1 << 10)
    mov cr4, eax
    mov dword [mxcsr_tmp], 0x1F80 & ~(1 << 9)
    ldmxcsr [mxcsr_tmp]
    TRAP_BEGIN
        movss xmm0, [one]
        divss xmm0, [zero]
    TRAP_END 3, 6
    ; reserved MXCSR bits => #GP
    TRAP_BEGIN
        mov dword [mxcsr_tmp], 0x80000000
        ldmxcsr [mxcsr_tmp]
    TRAP_END 4, 13
    PASS
align 16
mxcsr_tmp: dd 0
zero: dd 0.0
one:  dd 1.0
