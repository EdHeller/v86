; EXPECT: pass
; DESC: 32-bit JIT, page anchors (codegen::gen_access_anchor): accesses at a register plus a small displacement test the TLB entry of the register's page, loaded once per basic block. With paging, two neighbouring virtual pages mapped to unrelated physical pages (not a swap: a TLB entry holds physical ^ virtual, through which the swapped page's address would come out right): a register moved into the next page between two accesses, a register too near its page's end (an access in the next page), the window's edges (reg & 0xFFF = 0x80 and 0xF80, displacements -128 and +124), a write through an anchored register into a page with compiled code (once, halfway), and the stack: 40 pushes from esp = V4 + 0x80 (the window's edge) into V3, then [esp+156] and [esp] (anchors follow esp through push/pop: codegen::note_stack_adjust), checked at the physical pages
%include "inc/pm32.inc"

%define N 200000
%define PD 0x30000
%define PT 0x31000
%define V1 0x200000
%define V2 0x201000
%define P1 0x300000
%define P2 0x305000
%define V3 0x202000
%define V4 0x203000
%define P3 0x30A000
%define P4 0x30F000

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
    mov dword [PT + (V3 >> 12) * 4], P3 | 3
    mov dword [PT + (V4 >> 12) * 4], P4 | 3
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
    call case_stack
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
    mov eax, [sums + 16]
    mov [RESULT_ACT], eax
    CHECK16 5, eax, (N * (N + 1) / 2) & 0xFFFFFFFF
    mov eax, [sums + 20]
    mov [RESULT_ACT], eax
    CHECK16 6, eax, (N * (N + 1) / 2) & 0xFFFFFFFF
    ; where the last iteration's pushes (ebp = 1) went, by physical address: the first at V4 + 0x7C, the 33rd at
    ; V3 + 0xFFC, the 40th at V3 + 0xFE0
    mov eax, [P4 + 0x7C]
    mov [RESULT_ACT], eax
    CHECK16 7, eax, 1
    mov eax, [P3 + 0xFFC]
    mov [RESULT_ACT], eax
    CHECK16 8, eax, 1
    mov eax, [P3 + 0xFE0]
    mov [RESULT_ACT], eax
    CHECK16 9, eax, 1
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

align 4096
case_stack:
    mov [saved_esp], esp
    mov esp, V4 + 0x80
    ; (ebp, the iteration: values that differ each time, or the interpreted iterations' would hide wrong writes)
%rep 40
    push ebp
%endrep
    mov ecx, [esp + 156]
    add [sums + 16], ecx
    mov ecx, [esp]
    add [sums + 20], ecx
    add esp, 160
    mov esp, [saved_esp]
    ret

align 4096
; (in a page of their own: writes to a page with code invalidate it, and it would never be compiled)
sums: times 6 dd 0
saved_esp: dd 0
