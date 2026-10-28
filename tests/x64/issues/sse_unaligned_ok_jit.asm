; EXPECT: pass
; DESC: unaligned 16-byte SSE accesses that are legal (movups/movupd/movdqu loads and stores, lddqu) never fault, in interpreted and compiled code; the aligned-only forms still do (a jit regression: movups/movdqu stores were compiled as movaps and a 32-bit Linux userland crashed in memcpy)
%include "inc/pm32.inc"

%macro SSE_ON 0
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, 1 << 1
    mov cr0, eax
    mov eax, cr4
    or eax, (1 << 9) | (1 << 10)
    mov cr4, eax
%endmacro

%macro COPY_ALL 0
    ; unaligned loads and stores through registers
    movups xmm0, [esi + 1]
    movups [edi + 3], xmm0
    movupd xmm1, [esi + 5]
    movupd [edi + 21], xmm1
    movdqu xmm2, [esi + 7]
    movdqu [edi + 39], xmm2
    lddqu xmm3, [esi + 9]
    movdqu [edi + 57], xmm3
    movups [edi + 1], xmm0
    movupd [edi + 75], xmm1
%endmacro

test_main:
    SSE_ON
    ; the source holds bytes 0, 1, 2, ...
    xor ebx, ebx
.fill:
    mov [src + ebx], bl
    inc ebx
    cmp ebx, 96
    jb .fill
    ; run the sequence cold, then long enough to be compiled
    mov esi, src
    mov edi, dst
    mov ecx, 40000
.loop:
    COPY_ALL
    dec ecx
    jnz .loop
    CHECK16 1, dword [EXC_VEC], -1              ; nothing faulted
    CHECK16 2, byte [dst + 3], 3                ; movups [di+1] wrote src[1..] last
    CHECK16 3, byte [dst + 18], 16
    CHECK16 4, byte [dst + 21], 5               ; movupd
    CHECK16 5, byte [dst + 39], 7               ; movdqu
    CHECK16 6, byte [dst + 57], 9               ; lddqu
    ; the aligned-only forms fault on a misaligned address, also when compiled
    mov ecx, 8
.again:
    TRAP_BEGIN
        movaps [edi + 4], xmm0
    TRAP_END 7, 13
    TRAP_BEGIN
        movapd [edi + 4], xmm0
    TRAP_END 8, 13
    TRAP_BEGIN
        movdqa [edi + 4], xmm0
    TRAP_END 9, 13
    TRAP_BEGIN
        movntdq [edi + 4], xmm0
    TRAP_END 10, 13
    TRAP_BEGIN
        movdqa xmm0, [esi + 4]
    TRAP_END 11, 13
    dec ecx
    jz .done
    jmp .again
.done:
    PASS
align 16
src: times 128 db 0
dst: times 128 db 0
