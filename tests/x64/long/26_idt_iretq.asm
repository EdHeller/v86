; EXPECT: xfail
; DESC: int/iretq/exceptions in long mode: 16-byte IDT gates, 5-qword frames, error codes
%include "inc/long.inc"

test_main:
    TRAP_BEGIN
        ud2
    TRAP_END 1, 6
    TRAP_BEGIN
        int3
    TRAP_END 2, 3
    TRAP_BEGIN
        xor ecx, ecx
        div rcx
    TRAP_END 3, 0
    TRAP_BEGIN
        int 0x04                                ; software interrupt through the gate
    TRAP_END 4, 4
    TRAP_BEGIN
        mov rax, cr3
        mov cr3, rax                            ; valid
        mov ax, 0x1000
        mov ds, ax                              ; selector beyond the GDT limit -> #GP
    TRAP_END 5, 13
    ; manual iretq frame: ss, rsp, rflags, cs, rip
    lea rbx, [rel .landed]
    mov rcx, rsp
    push qword 0x10
    push rcx
    pushfq
    push qword 0x08
    push rbx
    iretq
    mov eax, 0x66
    CHECK_EQ 6, rax, 0
.landed:
    CHECK_EQ 7, rsp, rcx
    ; the exception frame holds the rip of the faulting instruction (fault: ud2)...
    lea rax, [rel .ud_done]
    mov [EXC_RESUME], rax
    lea r8, [rel .ud_site]
.ud_site:
    ud2
.ud_done:
    mov qword [EXC_RESUME], 0
    CHECK_EQ 8, qword [EXC_RIP], r8
    ; ...and the rip after the instruction for traps (int3)
    lea rax, [rel .after_int3]
    mov [EXC_RESUME], rax
    int3
.after_int3:
    mov qword [EXC_RESUME], 0
    CHECK_EQ 9, qword [EXC_RIP], rax
    PASS
