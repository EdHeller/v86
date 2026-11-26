; EXPECT: pass
; DESC: 32-bit JIT: rep string instructions that finish in one call go on in the compiled block (rep movs/stos/scas/cmps: registers, memory, flags after them), and leave it when they stop at a page's end (a copy across a page boundary), have no work (ecx 0) or rewrite the code right after them (rep movsb over the next instruction, once, in compiled code)
%include "inc/pm32.inc"

%define N 200000

test_main:
    mov ebp, N
.rep:
    call body_movs
    call body_stos
    call body_scas
    call body_cmps
    call body_cross
    call body_zero
    call body_smc
    dec ebp
    jnz .rep
    CHECK16 1, dword [sums + 0], 0
    CHECK16 2, dword [sums + 4], 37 * N
    CHECK16 3, dword [sums + 8], 37 * N
    CHECK16 4, dword [sums + 12], (N * (N + 1) / 2) & 0xFFFFFFFF
    CHECK16 5, dword [sums + 16], 40 * N
    CHECK16 6, dword [sums + 20], 77 * N
    CHECK16 7, dword [sums + 24], 27 * N
    CHECK16 8, dword [sums + 28], 30 * N
    CHECK16 9, dword [sums + 32], 0
    CHECK16 10, dword [sums + 36], N / 2 * 1 + N / 2 * 2
    CHECK16 11, dword [bad], 0
    PASS

align 4096
body_movs:
    mov esi, src
    mov edi, dst
    mov ecx, 37
    rep movsb
    add [sums + 0], ecx
    sub esi, src
    add [sums + 4], esi
    movzx eax, byte [dst + 36]
    add [sums + 8], eax
    mov byte [dst + 36], 0
    ret

align 4096
body_stos:
    mov edi, dst2
    mov ecx, 10
    mov eax, ebp
    rep stosd
    mov eax, [dst2 + 36]
    add [sums + 12], eax
    sub edi, dst2
    add [sums + 16], edi
    ret

align 4096
body_scas:
    mov edi, src
    mov ecx, 100
    mov al, 23
    repne scasb
    je .found
    add dword [bad], 1
.found:
    add [sums + 20], ecx
    ret

align 4096
body_cmps:
    mov esi, src
    mov edi, src2
    mov ecx, 37
    repe cmpsb
    jne .differs
    add dword [bad], 1
.differs:
    add [sums + 24], ecx
    ret

align 4096
body_cross:
    mov esi, src
    mov edi, cross_dst
    mov ecx, 30
    rep movsb
    movzx eax, byte [cross_dst + 29]
    add [sums + 28], eax
    mov byte [cross_dst + 29], 0
    ret

align 4096
body_zero:
    mov esi, src
    mov edi, dst
    xor ecx, ecx
    rep movsb
    sub edi, dst
    add [sums + 32], edi
    add [sums + 32], ecx
    ret

align 4096
body_smc:
    ; (copies nothing but once, halfway: a write each time would keep the page from being compiled)
    xor ecx, ecx
    cmp ebp, N / 2
    jne .copy
    mov ecx, 5
.copy:
    mov esi, patch
    mov edi, .target
    rep movsb
.target:
    mov eax, 1
    add [sums + 36], eax
    ret
patch: db 0xB8, 2, 0, 0, 0

align 4096
src:
%assign i 1
%rep 100
    db i
%assign i i+1
%endrep
src2:
%assign i 1
%rep 37
%if i == 10
    db 0
%else
    db i
%endif
%assign i i+1
%endrep
align 64
dst: times 64 db 0
dst2: times 64 db 0
sums: times 10 dd 0
bad: dd 0
align 4096
    times 4096 - 10 db 0
cross_dst: times 64 db 0
