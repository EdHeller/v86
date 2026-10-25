; EXPECT: pass
; ISSUE: copy/v86#1631
; DESC: no interrupt is taken between MOV SS and the following instruction
; Statistical: PIT channel 0 fires IRQ0 thousands of times while the main loop
; executes `mov ss, ax / mov esp, ebx` pairs. The IRQ0 handler fails the test if the
; interrupted eip is the instruction directly after a `mov ss`.
%include "inc/pm32.inc"

%define TICKS 0x700

test_main:
    ; route PIC interrupts to the cpu through the local APIC's LINT0 (ExtINT, unmasked)
    mov dword [0xFEE00350], 0x700
    ; master PIC: vectors 0x20-0x27, only IRQ0 unmasked
    mov al, 0x11
    out 0x20, al
    mov al, 0x20
    out 0x21, al
    mov al, 0x04
    out 0x21, al
    mov al, 0x01
    out 0x21, al
    mov al, 0xFE
    out 0x21, al
    mov al, 0xFF
    out 0xA1, al
    ; gate for vector 0x20 (the idt built by pm32.inc covers 0-31 only)
    mov edi, IDT_BASE + 0x20 * 8
    mov eax, irq0
    mov word [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov word [edi + 6], ax
    mov word [idt_ptr], 0x21 * 8 - 1
    lidt [idt_ptr]
    ; PIT channel 0, mode 2 (rate generator), ~6 kHz
    mov al, 0x34
    out 0x43, al
    mov al, 200
    out 0x40, al
    xor al, al
    out 0x40, al
    mov dword [TICKS], 0
    mov ax, 0x10
    mov ebx, STACK32
    sti
.loop:
%rep 16
    mov ss, ax
    mov esp, ebx
    jmp short $ + 2                             ; a legal interrupt point (also ends a qemu TB)
%endrep
    cmp dword [TICKS], 400
    jb .loop
    cli
    PASS

irq0:
    push eax
    push ebx
    mov eax, [esp + 8]                          ; interrupted eip
    inc dword [TICKS]
    ; every `mov esp, ebx` (89 DC) directly follows a `mov ss, ax` (8E D0)
    cmp word [eax - 2], 0xD08E
    jne .ok
    mov dword [RESULT_ID], 1
    mov [RESULT_ACT], eax
    mov dword [RESULT], STATUS_FAIL
    hlt
.ok:
    mov al, 0x20
    out 0x20, al
    pop ebx
    pop eax
    iretd
