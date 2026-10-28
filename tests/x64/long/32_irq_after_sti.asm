; EXPECT: pass
; DESC: a pending interrupt is delivered right after sti / popfq / iretq enable them (not only at the poll between instruction batches): TempleOS' scheduler loop starved its timer interrupt otherwise
%include "inc/long.inc"

%define TICKS 0x700

; IRQ0 from the PIT; the handler counts it and sends the EOI
irq0:
    push rax
    inc qword [TICKS]
    mov al, 0x20
    out 0x20, al
    pop rax
    iretq

; with interrupts off, let the PIT fire so that IRQ0 is pending on the PIC
%macro LET_IRQ_PEND 0
    mov ecx, 800000
%%spin:
    dec ecx
    jnz %%spin
%endmacro

test_main:
    ; PIC: vector base 0x20, only IRQ0 unmasked
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
    ; IDT gate 0x20
    lea rax, [rel irq0]
    mov edi, IDT_BASE + 0x20 * 16
    mov word [rdi], ax
    mov word [rdi + 2], 0x08
    mov word [rdi + 4], 0x8E00
    shr eax, 16
    mov word [rdi + 6], ax
    mov dword [rdi + 8], 0
    mov dword [rdi + 12], 0
    lea rax, [rel idt_big]
    lidt [rax]
    ; PIT channel 0, mode 2, about 1 kHz
    mov al, 0x34
    out 0x43, al
    mov ax, 1193
    out 0x40, al
    mov al, ah
    out 0x40, al
    mov qword [TICKS], 0
    cli

    ; sti: delivered after the next instruction
    LET_IRQ_PEND
    mov rbx, [TICKS]
    sti
    nop
    nop
    nop
    mov rax, [TICKS]
    cli
    sub rax, rbx
    xor edx, edx
    cmp rax, 1
    setae dl
    CHECK_EQ 1, rdx, 1

    ; popfq with IF set
    LET_IRQ_PEND
    mov rbx, [TICKS]
    pushfq
    or qword [rsp], 0x200
    popfq
    nop
    nop
    mov rax, [TICKS]
    cli
    sub rax, rbx
    xor edx, edx
    cmp rax, 1
    setae dl
    CHECK_EQ 2, rdx, 1

    ; iretq to a frame with IF set
    LET_IRQ_PEND
    mov rbx, [TICKS]
    lea rsi, [rel .landed]
    mov rcx, rsp
    push qword 0x10
    push rcx
    pushfq
    or qword [rsp], 0x200
    push qword 0x08
    push rsi
    iretq
.landed:
    nop
    nop
    mov rax, [TICKS]
    cli
    sub rax, rbx
    xor edx, edx
    cmp rax, 1
    setae dl
    CHECK_EQ 3, rdx, 1
    PASS

align 8
idt_big:
    dw 0x21 * 16 - 1
    dq IDT_BASE
