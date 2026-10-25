; EXPECT: xfail
; QEMU: xfail
; DESC: #GP(0) for non-canonical MSR bases, reserved EFER bits, clearing LME under paging, reserved CR8 bits
; (QEMU's TCG skips these checks, so this test is not validated against it.)
%include "inc/long.inc"

test_main:
    ; non-canonical values are rejected with #GP(0)
    TRAP_BEGIN
        mov ecx, 0xC0000100
        xor eax, eax
        mov edx, 0x00008000                     ; 0x0000_8000_0000_0000
        wrmsr
    TRAP_END 1, 13
    TRAP_BEGIN
        mov ecx, 0xC0000082
        xor eax, eax
        mov edx, 0x00008000
        wrmsr
    TRAP_END 2, 13
    ; reserved EFER bits => #GP
    TRAP_BEGIN
        mov ecx, 0xC0000080
        mov eax, 1 << 20
        xor edx, edx
        wrmsr
    TRAP_END 3, 13
    ; EFER.LME cannot be cleared while paging is enabled
    TRAP_BEGIN
        mov ecx, 0xC0000080
        xor eax, eax
        xor edx, edx
        wrmsr
    TRAP_END 4, 13
    ; reserved bits in CR8 (only bits 3:0 exist) => #GP
    mov rax, 16
    TRAP_BEGIN
        mov cr8, rax
    TRAP_END 5, 13
    PASS
