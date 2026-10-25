; EXPECT: pass
; DESC: 4-level paging: 4K page remapping, alias mappings, NX bit, user/supervisor, write protect, invlpg
%include "inc/long.inc"

test_main:
    ; Build: PML4[0] -> PDPT[8] -> PD2[0] -> PT2, i.e. linear 0x2_0000_0000
    ; maps 4K pages onto physical 0x300000 / 0x301000.
    %define PD2 0x5000
    %define PT2 0x6000
    %define ALIAS 0x200000000
    mov edi, PD2
    xor eax, eax
    mov ecx, 2 * 4096 / 8
    rep stosq
    mov dword [PDPT + 8 * 8], PD2 | 7
    mov dword [PD2], PT2 | 7
    mov dword [PT2], 0x300000 | 7               ; page 0: rw, user
    mov dword [PT2 + 8], 0x301000 | 5           ; page 1: read-only, user
    mov rax, cr3
    mov cr3, rax
    mov rbx, ALIAS
    mov rax, 0x1122334455667788
    mov [rbx], rax
    CHECK_EQ 1, qword [0x300000], 0x1122334455667788  ; visible through the identity mapping
    ; remap page 0 to physical 0x301000 and flush with invlpg
    mov qword [0x301000], 0x99
    mov dword [PT2], 0x301000 | 7
    invlpg [rbx]
    mov rax, [rbx]
    CHECK_EQ 2, rax, 0x99
    ; write to a read-only page faults even at CPL0 when CR0.WP is set
    mov rax, cr0
    or rax, 1 << 16
    mov cr0, rax
    mov rbx, ALIAS + 0x1000
    TRAP_BEGIN
        mov qword [rbx], 1
    TRAP_END 3, 14
    CHECK_EQ 4, qword [EXC_ERR], 3                    ; present + write
    mov rax, cr2
    CHECK_EQ 5, rax, ALIAS + 0x1000
    ; without WP the supervisor may write
    mov rax, cr0
    and rax, ~(1 << 16)
    mov cr0, rax
    mov qword [rbx], 0x42
    CHECK_EQ 6, qword [0x301000], 0x42
    ; NX: enable EFER.NXE and make page 0 non-executable
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 11
    wrmsr
    mov byte [0x301000], 0xC3                   ; ret
    mov rax, 0x8000000000000000
    mov rdx, 0x301000 | 7
    or rdx, rax
    mov [PT2], rdx
    invlpg [ALIAS]
    mov rbx, ALIAS
    TRAP_BEGIN
        call rbx
    TRAP_END 7, 14
    CHECK_EQ 8, qword [EXC_ERR], 0x11                 ; present + instruction fetch
    ; clearing NX makes the same page executable again
    mov dword [PT2], 0x301000 | 7
    mov dword [PT2 + 4], 0
    invlpg [ALIAS]
    call rbx
    ; reserved bit set in a PTE -> #PF with RSVD error bit
    mov dword [PT2 + 8 * 2], 0x302000 | 7
    mov dword [PT2 + 8 * 2 + 4], 0x00080000     ; bit 51: reserved unless MAXPHYADDR is 52
    invlpg [ALIAS + 0x2000]
    mov rbx, ALIAS + 0x2000
    TRAP_BEGIN
        mov rax, [rbx]
    TRAP_END 9, 14
    mov rax, [EXC_ERR]
    and eax, 8                                  ; RSVD (hardware also reports P=1, i.e. 9, but qemu reports 8)
    CHECK_EQ 10, rax, 8
    PASS
