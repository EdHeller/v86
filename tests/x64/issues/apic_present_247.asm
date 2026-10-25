; EXPECT: pass
; ISSUE: copy/v86#247
; V86: {"acpi": true}
; DESC: local APIC: CPUID.1:EDX.APIC, IA32_APIC_BASE, MMIO id/version/TPR/spurious-vector registers at 0xFEE00000
%include "inc/pm32.inc"

test_main:
    mov eax, 1
    cpuid
    test edx, 1 << 9
    jnz .cpuid_apic
    FAIL16 1
.cpuid_apic:
    mov ecx, 0x1B                               ; IA32_APIC_BASE
    rdmsr
    mov ebx, eax
    and ebx, 0xFFFFF000
    cmp ebx, 0xFEE00000
    je .base
    FAIL16 2
.base:
    test eax, 1 << 11                           ; global enable
    jnz .enabled
    FAIL16 3
.enabled:
    test eax, 1 << 8                            ; BSP
    jnz .bsp
    FAIL16 4
.bsp:
    mov eax, [0xFEE00030]                       ; version register
    and eax, 0xFF
    cmp eax, 0x10
    jae .ver_lo
    FAIL16 5
.ver_lo:
    cmp eax, 0x15
    jb .ver_hi
    FAIL16 6
.ver_hi:
    mov dword [0xFEE00080], 0x20                ; TPR round trip
    CHECK16 7, dword [0xFEE00080], 0x20
    mov dword [0xFEE00080], 0
    mov dword [0xFEE000F0], 0x1FF               ; spurious interrupt vector register, enable
    mov eax, [0xFEE000F0]
    and eax, 0x1FF
    CHECK16 8, eax, 0x1FF
    PASS
