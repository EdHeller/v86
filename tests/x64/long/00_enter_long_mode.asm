; EXPECT: xfail
; ISSUE: copy/v86#133
; DESC: switch to long mode (EFER.LME + PAE paging), land in 64-bit code, EFER.LMA set
%include "inc/long.inc"

test_main:
    mov ecx, 0xC0000080
    rdmsr
    test eax, 1 << 10                   ; LMA
    jnz .lma
    mov eax, 1
    xor r15, r15
    call report_fail
.lma:
    mov rax, 0x1122334455667788
    CHECK_EQ 2, rax, 0x1122334455667788
    PASS
