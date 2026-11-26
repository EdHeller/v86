; EXPECT: pass
; DESC: 32-bit JIT, page anchors (codegen::gen_access_anchor): accesses at a register plus a small displacement test the TLB entry of the register's page, loaded once per basic block. With paging, two neighbouring virtual pages mapped to unrelated physical pages (not a swap: a TLB entry holds physical ^ virtual, through which the swapped page's address would come out right): a register moved into the next page between two accesses, a register too near its page's end (an access in the next page), the window's edges (reg & 0xFFF = 0x80 and 0xF80, displacements -128 and +124), and a write through an anchored register into a page with compiled code (once, halfway)
%include "inc/pm32.inc"

%define N 200000
%define PD 0x30000
%define PT 0x31000
%define V1 0x200000
%define V2 0x201000
%define P1 0x300000
%define P2 0x305000

test_main:
    ; identity map the first 4 MiB but V1 -> P1, V2 -> P2
    mov edi, PT
    mov eax, 3
    mov ecx, 1024
.pte:
    mov [edi], eax
    add eax, 0x1000
    add edi, 4
    loop .pte
    mov dword [PT + (V1 >> 12) * 4], P1 | 3
    mov dword [PT + (V2 >> 12) * 4], P2 | 3
    mov dword [PD], PT | 3
    ; the data, by physical address (paging off): what the virtual addresses will show
    mov dword [P1 + 0xF00], 0x10        ; V1 + 0xF00
    mov dword [P2 + 0x100], 0x20        ; V2 + 0x100
    mov dword [P1 + 0xFF8], 0x30        ; V1 + 0xFF8
    mov dword [P2 + 0x000], 0x40        ; V2 + 0x000
    mov dword [P1 + 0x000], 5           ; V1 + 0x000
    mov dword [P1 + 0x0FC], 7           ; V1 + 0x0FC
    mov dword [P1 + 0xFFC], 9           ; V1 + 0xFFC
    mov eax, PD
    mov cr3, eax
    mov eax, cr0
    or eax, 0x80000000
    mov cr0, eax
    jmp .paged
.paged:
    mov ebp, N
.rep:
    call case_move
    call case_edge
    call case_window
    call case_smc
    dec ebp
    jnz .rep
    mov eax, [sums + 0]
    mov [RESULT_ACT], eax
    CHECK16 1, eax, N * (0x10 + 2 * 0x20)
    mov eax, [sums + 4]
    mov [RESULT_ACT], eax
    CHECK16 2, eax, N * (0x30 + 2 * 0x40)
    mov eax, [sums + 8]
    mov [RESULT_ACT], eax
    CHECK16 3, eax, N * (5 + 2 * 7 + 3 * 9 + 4 * 0x10)
    mov eax, [sums + 12]
    mov [RESULT_ACT], eax
    CHECK16 4, eax, N / 2 * 1 + N / 2 * 2
    PASS

align 4096
case_move:
    ; (an anchor for ebx, then ebx in the next page: its accesses must not use the anchor)
    mov ebx, V1 + 0xF00
    mov eax, [ebx]
    add ebx, 0x200
    mov ecx, [ebx]
    add eax, ecx
    add eax, ecx
    add [sums + 0], eax
    ret

align 4096
case_edge:
    ; (ebx 16 bytes before its page's end: [ebx+16] is in the next page)
    mov ebx, V1 + 0xFF0
    mov eax, [ebx + 8]
    mov ecx, [ebx + 16]
    add eax, ecx
    add eax, ecx
    add [sums + 4], eax
    ret

align 4096
case_window:
    mov ebx, V1 + 0x80
    mov eax, [ebx - 128]
    mov ecx, [ebx + 124]
    add eax, ecx
    add eax, ecx
    mov ebx, V1 + 0xF80
    mov ecx, [ebx + 124]
    lea ecx, [ecx * 3]
    add eax, ecx
    mov ecx, [ebx - 128]
    shl ecx, 2
    add eax, ecx
    add [sums + 8], eax
    ret

align 4096
case_smc:
    ; (writes through edi, anchored by the read before it, into this page once, halfway: the patched
    ; instruction must run from then on)
    mov edi, .target - 4
    cmp ebp, N / 2
    jne .skip
    mov ecx, [edi]
    mov eax, 2
    mov [edi + 5], eax
.skip:
    nop
    nop
    nop
    nop
.target:
    mov eax, 1
    add [sums + 12], eax
    ret

align 64
sums: times 4 dd 0
