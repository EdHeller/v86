; Environment independent checking macros for unprivileged 64-bit code.
; Build with -DNATIVE to run as a Linux ELF64 for validating expectations,
; otherwise the code is assumed to run in 64-bit mode inside v86 (see
; inc/long.inc, which supplies the mode switch and `report_fail`).
;
; Clobbers r15 (and rax/rdx/rdi/rsi/rcx/r11 in the native failure path).

%macro CHECK_EQ 3          ; id, 64-bit register or [mem qword], imm64/reg
    %ifnum %3
      %if %3 > 0x7FFFFFFF || %3 < -0x80000000
        mov r15, %3
        cmp %2, r15
      %else
        cmp %2, %3
      %endif
    %else
        cmp %2, %3
    %endif
    je %%ok
    mov r15, %2
    mov rax, %1
    call report_fail       ; r15 = actual (rdx = expected unused), eax = id
%%ok:
%endmacro

; Compare only the low 32 bits of operand against imm32
%macro CHECK_EQ32 3
    cmp %2, %3
    je %%ok
    mov r15d, %2
    mov rax, %1
    call report_fail
%%ok:
%endmacro

; Continue only if condition code `cc` holds, e.g. CHECK_CC 3, z / CHECK_CC 4, nc
%macro CHECK_CC 2          ; id, cc
    j%2 %%ok
    mov rax, %1
    xor r15, r15
    call report_fail
%%ok:
%endmacro
