; EXPECT: xfail
; DESC: BOUND raises #BR (int 5) when the index is outside [lower, upper], else falls through
%include "inc/real.inc"

test_main:
    mov ax, 15
    bound ax, [limits]                          ; in range: no exception
    CHECK16 1, dword [EXC_VEC], -1
    mov ax, 10
    bound ax, [limits]                          ; bounds are inclusive
    mov ax, 20
    bound ax, [limits]
    CHECK16 2, dword [EXC_VEC], -1
    TRAP_BEGIN
        mov ax, 9
        bound ax, [limits]
    TRAP_END 3, 5
    TRAP_BEGIN
        mov ax, 21
        bound ax, [limits]
    TRAP_END 4, 5
    ; 32-bit form: bound eax, [mem] with 2 dwords
    TRAP_BEGIN
        mov eax, -1
        bound eax, [limits32]                   ; limits32 = [0, 100]
    TRAP_END 5, 5
    mov eax, 100
    bound eax, [limits32]
    PASS
limits:   dw 10, 20
limits32: dd 0, 100
