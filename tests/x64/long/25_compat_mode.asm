; EXPECT: xfail
; DESC: compatibility mode: far jump to a 32-bit code segment under LMA, 32-bit semantics, return to 64-bit
%include "inc/long.inc"

test_main:
    ; retfq style far jump into compat mode code at selector 0x18
    lea rax, [rel compat]
    push qword 0x18
    push rax
    retfq
bits 32
compat:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov eax, 0x7FFFFFFF
    inc eax                                     ; 0x40 is INC EAX here, not REX
    CHECK16 1, eax, 0x80000000
    mov ebx, esp
    pusha                                       ; valid in compat mode
    popa
    CHECK16 2, esp, ebx
    push eax
    cmp esp, ebx
    jne .esp_moved
    FAIL16 3
.esp_moved:
    pop eax
    mov ecx, ebx
    sub ecx, esp
    CHECK16 4, ecx, 0
    mov eax, 0x12345678
    mov edx, 0xFFFFFFFF
    add edx, 1
    jc .carry32                                 ; 32-bit carry, not 64-bit
    FAIL16 5
.carry32:
    ; back to 64-bit mode via far jump with a 32-bit offset
    jmp 0x08:back64
bits 64
back64:
    mov ax, 0x10
    mov ss, ax
    mov rax, 0x1122334455667788
    CHECK_EQ 6, rax, 0x1122334455667788
    PASS
