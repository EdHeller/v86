; EXPECT: pass
; V86: {"jit64_threshold": 5, "sync_jit": true}
; DESC: jit64: faults inside compiled blocks leave the architectural state of the faulting instruction
%include "inc/long.inc"

test_main:
    ; page faults (read and write) in the middle of a block, repeated until the block is compiled
    xor r9d, r9d
    xor r12d, r12d
    mov rbx, 0x4000000                  ; beyond the 32MB identity map
.pf_loop:
    xor r8d, r8d
    TRAP_BEGIN
        mov r8d, 1
        add r9d, 1
.pf_read:
        mov rax, [rbx]
        mov r8d, 2                      ; never reached
    TRAP_END 1, 14
    CHECK_EQ 2, r8, 1
    lea rax, [rel .pf_read]
    CHECK_EQ 3, qword [EXC_RIP], rax
    mov rax, cr2
    CHECK_EQ 4, rax, 0x4000000
    TRAP_BEGIN
        mov r8d, 3
.pf_write:
        mov [rbx + 8], r8
        mov r8d, 4                      ; never reached
    TRAP_END 5, 14
    CHECK_EQ 6, r8, 3
    lea rax, [rel .pf_write]
    CHECK_EQ 7, qword [EXC_RIP], rax
    CHECK_EQ 8, qword [EXC_ERR], 2      ; not present, write
    inc r12d
    cmp r12d, 40
    jb .pf_loop
    CHECK_EQ 9, r9, 40

    ; general protection faults: non-canonical address
    mov rbx, 0x0001000000000000
    xor r12d, r12d
.gp_loop:
    mov r10, 7
    TRAP_BEGIN
        add r10, 1
.gp_insn:
        mov [rbx], r10
        add r10, 100                    ; never reached
    TRAP_END 10, 13
    CHECK_EQ 11, r10, 8
    lea rax, [rel .gp_insn]
    CHECK_EQ 12, qword [EXC_RIP], rax
    inc r12d
    cmp r12d, 40
    jb .gp_loop

    PASS
