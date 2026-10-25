; EXPECT: pass
; DESC: page faults and general protection for non-canonical addresses in long mode
%include "inc/long.inc"

test_main:
    mov rbx, 0x0000800000000000                 ; first non-canonical address
    TRAP_BEGIN
        mov rax, [rbx]
    TRAP_END 1, 13                              ; #GP
    CHECK_EQ 2, qword [EXC_ERR], 0
    mov rbx, 0xFFFF7FFFFFFFFFFF                 ; last non-canonical address
    TRAP_BEGIN
        mov [rbx], rax
    TRAP_END 3, 13
    ; (a non-canonical rsp raises #SS, but needs an IST stack to be handled; not tested here)
    ; canonical but not mapped -> #PF, CR2 = address, error code = not present/read/supervisor
    mov rbx, 0x00007FFF00000000
    TRAP_BEGIN
        mov rax, [rbx]
    TRAP_END 5, 14
    mov rax, cr2
    CHECK_EQ 6, rax, 0x00007FFF00000000
    CHECK_EQ 7, qword [EXC_ERR], 0
    mov rbx, 0xFFFF800000000000                 ; lowest canonical high-half address, unmapped
    TRAP_BEGIN
        mov qword [rbx], 1
    TRAP_END 8, 14
    CHECK_EQ 9, qword [EXC_ERR], 2                    ; write, not present
    ; lea / jump targets are not memory accesses: lea with a non-canonical result is fine
    mov rbx, 0x0000800000000000
    lea rax, [rbx + 8]
    CHECK_EQ 10, rax, 0x0000800000000008
    ; jumping to a non-canonical rip raises #GP
    TRAP_BEGIN
        jmp rbx
    TRAP_END 11, 13
    PASS
