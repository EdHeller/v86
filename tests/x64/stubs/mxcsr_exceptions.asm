; EXPECT: pass
; DESC: MXCSR sticky exception flags with masked exceptions (IEEE default results)
%include "inc/real.inc"

test_main:
    SSE_ON                                      ; includes CR4.OSXMMEXCPT
    ; all exceptions masked: results are the IEEE defaults and flags are sticky
    mov dword [mxcsr_tmp], 0x1F80
    ldmxcsr [mxcsr_tmp]
    movss xmm0, [one]
    divss xmm0, [zero]
    movd eax, xmm0
    CHECK16 1, eax, 0x7F800000                  ; +inf
    stmxcsr [mxcsr_tmp]
    mov eax, [mxcsr_tmp]
    and eax, 0x3F
    CHECK16 2, eax, 0x04                        ; ZE
    movss xmm0, [zero]
    divss xmm0, [zero]
    movd eax, xmm0
    CHECK16 3, eax, 0xFFC00000                  ; default QNaN
    stmxcsr [mxcsr_tmp]
    mov eax, [mxcsr_tmp]
    and eax, 0x3F
    CHECK16 4, eax, 0x05                        ; ZE | IE (sticky)
    movss xmm0, [one]
    divss xmm0, [three]
    stmxcsr [mxcsr_tmp]
    mov eax, [mxcsr_tmp]
    and eax, 0x20
    CHECK16 5, eax, 0x20                        ; PE
    PASS
align 16
mxcsr_tmp: dd 0
zero: dd 0.0
one:  dd 1.0
three: dd 3.0
