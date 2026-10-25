; EXPECT: pass
; DESC: MXCSR.FTZ (bit 15) flushes denormal results, MXCSR.DAZ (bit 6) treats denormal inputs as zero
%include "inc/real.inc"

test_main:
    SSE_ON
    ; default: denormal results are produced
    mov dword [mxcsr_tmp], 0x1F80
    ldmxcsr [mxcsr_tmp]
    movss xmm0, [min_normal]
    mulss xmm0, [half]                          ; 2^-127 is denormal
    movd eax, xmm0
    CHECK16 1, eax, 0x00400000
    ; FTZ: the (masked underflow) result becomes +0
    mov dword [mxcsr_tmp], 0x1F80 | (1 << 15)
    ldmxcsr [mxcsr_tmp]
    movss xmm0, [min_normal]
    mulss xmm0, [half]
    movd eax, xmm0
    CHECK16 2, eax, 0
    ; DAZ clear: denormal input participates
    mov dword [mxcsr_tmp], 0x1F80
    ldmxcsr [mxcsr_tmp]
    movss xmm0, [denormal]
    mulss xmm0, [two]
    movd eax, xmm0
    CHECK16 3, eax, 0x00800000
    ; DAZ set: denormal input reads as zero
    mov dword [mxcsr_tmp], 0x1F80 | (1 << 6)
    ldmxcsr [mxcsr_tmp]
    movss xmm0, [denormal]
    mulss xmm0, [two]
    movd eax, xmm0
    CHECK16 4, eax, 0
    PASS
align 16
mxcsr_tmp: dd 0
min_normal: dd 0x00800000
denormal:   dd 0x00400000
half:       dd 0.5
two:        dd 2.0
