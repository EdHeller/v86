; EXPECT: xfail
; DESC: MSRs: EFER, STAR/LSTAR/CSTAR/SFMASK, FS/GS base round trips, #GP for non-canonical bases
%include "inc/long.inc"

test_main:
    mov ecx, 0xC0000080                         ; EFER
    rdmsr
    test eax, 1 << 8                            ; LME
    jnz .lme
    mov eax, 1
    xor r15d, r15d
    call report_fail
.lme:
    test eax, 1 << 10                           ; LMA
    jnz .lma
    mov eax, 2
    xor r15d, r15d
    call report_fail
.lma:
    mov ecx, 0xC0000081                         ; STAR
    mov eax, 0x11111111
    mov edx, 0x00200008
    wrmsr
    rdmsr
    CHECK_EQ32 3, eax, 0x11111111
    CHECK_EQ32 4, edx, 0x00200008
    mov ecx, 0xC0000082                         ; LSTAR
    mov eax, 0x22222222
    mov edx, 0x00007FFF
    wrmsr
    rdmsr
    CHECK_EQ32 5, eax, 0x22222222
    CHECK_EQ32 6, edx, 0x00007FFF
    mov ecx, 0xC0000083                         ; CSTAR
    mov eax, 0x33333333
    mov edx, 0xFFFF8000
    wrmsr
    rdmsr
    CHECK_EQ32 7, eax, 0x33333333
    CHECK_EQ32 8, edx, 0xFFFF8000
    mov ecx, 0xC0000084                         ; SFMASK
    mov eax, 0x00004700
    xor edx, edx
    wrmsr
    rdmsr
    CHECK_EQ32 9, eax, 0x00004700
    mov ecx, 0xC0000100                         ; FS.base
    mov eax, 0x44444444
    mov edx, 0x00001234
    wrmsr
    rdmsr
    CHECK_EQ32 10, eax, 0x44444444
    CHECK_EQ32 11, edx, 0x00001234
    mov ecx, 0xC0000101                         ; GS.base
    mov eax, 0x55555555
    mov edx, 0xFFFF8765
    wrmsr
    rdmsr
    CHECK_EQ32 12, eax, 0x55555555
    CHECK_EQ32 13, edx, 0xFFFF8765
    mov ecx, 0xC0000102                         ; KernelGSBase
    mov eax, 0x66666666
    xor edx, edx
    wrmsr
    rdmsr
    CHECK_EQ32 14, eax, 0x66666666
    PASS
