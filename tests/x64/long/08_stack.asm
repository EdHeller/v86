; EXPECT: xfail
; NATIVE: yes
; DESC: push/pop/call/ret/leave/enter use 8-byte stack slots; push imm32 sign extends
%include "inc/long.inc"

test_main:
    mov rbx, rsp
    push rax
    mov rax, rsp
    sub rbx, rax
    CHECK_EQ 1, rbx, 8
    pop rax
    push qword -2
    pop rcx
    CHECK_EQ 2, rcx, 0xFFFFFFFFFFFFFFFE
    mov rax, 0x1122334455667788
    push rax
    CHECK_EQ 3, [rsp], 0x1122334455667788
    pop rdx
    push r15
    push r8
    pop r9
    pop r10
    nop
    push word 0x1234                            ; 16-bit push moves rsp by 2
    mov rax, rsp
    pop cx
    sub rax, rsp
    CHECK_EQ 5, rax, -2
    mov rbx, rsp
    call .f
    CHECK_EQ 6, rsp, rbx
    jmp .after_f
.f:
    nop
    mov rax, [rsp]
    lea rcx, [rel .ret_site]
    ret
.ret_site:
.after_f:
    mov rbp, rsp
    push rbp
    mov rbp, rsp
    sub rsp, 32
    leave
    mov rax, rbp
    sub rax, rsp
    CHECK_EQ 8, rax, 0
    PASS
