; EXPECT: pass
; ISSUE: copy/v86#1356
; DESC: alignment checks are also enforced by jit compiled code (the loop runs long enough to be compiled)
%include "inc/real.inc"

test_main:
    SSE_ON
    mov bx, buf
    mov cx, 3000
.loop:
    movaps xmm0, [bx]
    movdqa xmm1, [bx + 16]
    paddd xmm0, [bx + 32]
    movaps [bx + 48], xmm0
    dec cx
    jnz .loop
    CHECK16 1, dword [EXC_VEC], -1
    ; the same instructions with a misaligned operand fault, in the compiled code
    mov cx, 8
.again:
    TRAP_BEGIN
        movaps xmm0, [bx + 4]
    TRAP_END 2, 13
    TRAP_BEGIN
        paddd xmm0, [bx + 20]
    TRAP_END 3, 13
    TRAP_BEGIN
        movaps [bx + 8], xmm0
    TRAP_END 4, 13
    dec cx
    jz .again_done
    jmp .again
.again_done:
    ; run the faulting variant itself hot: aligned many times, then misaligned once
    mov cx, 3000
    mov si, buf
    TRAP_BEGIN
.hot:
    movdqa xmm2, [si]
    cmp cx, 1
    jne .keep
    add si, 4
.keep:
    dec cx
    jnz .hot
    movdqa xmm2, [si]
    TRAP_END 5, 13
    PASS
align 16
buf: times 128 db 1
