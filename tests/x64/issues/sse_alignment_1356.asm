; EXPECT: xfail
; ISSUE: copy/v86#1356
; DESC: legacy SSE instructions with 16-byte memory operands raise #GP on misaligned addresses
%include "inc/real.inc"


test_main:
    SSE_ON
    TRAP_BEGIN
        movaps xmm0, [buf + 4]
    TRAP_END 1, 13
    TRAP_BEGIN
        movaps [buf + 8], xmm0
    TRAP_END 2, 13
    TRAP_BEGIN
        movapd xmm0, [buf + 1]
    TRAP_END 3, 13
    TRAP_BEGIN
        movdqa xmm0, [buf + 15]
    TRAP_END 4, 13
    TRAP_BEGIN
        movdqa [buf + 2], xmm0
    TRAP_END 5, 13
    TRAP_BEGIN
        addps xmm0, [buf + 4]                   ; legacy SSE memory operands must be aligned
    TRAP_END 6, 13
    TRAP_BEGIN
        pxor xmm0, [buf + 8]
    TRAP_END 7, 13
    TRAP_BEGIN
        movntps [buf + 4], xmm0
    TRAP_END 8, 13
    TRAP_BEGIN
        movntdq [buf + 8], xmm0
    TRAP_END 9, 13
    mov dword [EXC_VEC], -1
    ; unaligned-tolerant forms must not fault
    movups xmm0, [buf + 4]
    movups [buf + 4], xmm0
    movupd xmm0, [buf + 1]
    movdqu xmm0, [buf + 3]
    movdqu [buf + 7], xmm0
    lddqu xmm0, [buf + 5]
    movlps xmm0, [buf + 3]                      ; 8-byte accesses are unconstrained
    movhps xmm0, [buf + 5]
    movsd xmm0, [buf + 3]
    movss xmm0, [buf + 1]
    movd xmm0, [buf + 1]
    CHECK16 12, dword [EXC_VEC], -1
    mov dword [EXC_VEC], -1
    ; aligned accesses work
    movaps xmm0, [buf]
    movaps [buf + 16], xmm0
    movdqa xmm0, [buf + 32]
    addps xmm0, [buf + 16]
    CHECK16 13, dword [EXC_VEC], -1
    PASS
align 16
buf: times 80 db 0
