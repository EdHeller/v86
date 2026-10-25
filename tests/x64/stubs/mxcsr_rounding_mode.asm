; EXPECT: pass
; DESC: MXCSR.RC rounding control is honoured by cvtss2si/cvtps2dq/cvtsd2si (cpu.rs: "Unimplemented MXCSR rounding mode")
%include "inc/real.inc"

%macro ROUND_CASE 5         ; id, rc, expect(2.5), expect(2.7), expect(-2.5)
    mov dword [mxcsr_tmp], 0x1F80 | (%2 << 13)
    ldmxcsr [mxcsr_tmp]
    stmxcsr [mxcsr_tmp + 4]
    CHECK16 (%1 << 4) | 0, dword [mxcsr_tmp + 4], 0x1F80 | (%2 << 13)
    cvtss2si eax, [v25]
    CHECK16 (%1 << 4) | 1, eax, %3
    cvtss2si eax, [v27]
    CHECK16 (%1 << 4) | 2, eax, %4
    cvtss2si eax, [vm25]
    CHECK16 (%1 << 4) | 3, eax, %5
    cvtps2dq xmm0, [vps]
    CHECK_XMM (%1 << 4) | 4, xmm0, %3, %4, %5, %4
    cvtsd2si eax, [vd27]
    CHECK16 (%1 << 4) | 5, eax, %4
%endmacro

test_main:
    SSE_ON
    ROUND_CASE 0, 0, 2, 3, -2                   ; nearest even
    ROUND_CASE 1, 1, 2, 2, -3                   ; down
    ROUND_CASE 2, 2, 3, 3, -2                   ; up
    ROUND_CASE 3, 3, 2, 2, -2                   ; toward zero
    ; truncating conversions ignore RC
    mov dword [mxcsr_tmp], 0x1F80 | (2 << 13)
    ldmxcsr [mxcsr_tmp]
    cvttss2si eax, [v27]
    CHECK16 0x50, eax, 2
    ; roundps with imm bit 2 set uses MXCSR.RC (round up here)
    roundps xmm0, [vps], 4
    CHECK_XMM 6, xmm0, 0x40400000, 0x40400000, 0xC0000000, 0x40400000
    PASS
align 16
mxcsr_tmp: dd 0, 0, 0, 0
v25:  dd 2.5
v27:  dd 2.7
vm25: dd -2.5
vd27: dq 2.7
align 16
vps:  dd 2.5, 2.7, -2.5, 2.7
