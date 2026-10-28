; EXPECT: pass
; V86: {"acpi": true, "cpu_cores": 3}
; DESC: three cores send each other broadcast IPIs (all but self, all, physical and logical) while they spin with core specific register values: interrupts and switches do not disturb registers
%include "inc/pm32.inc"

%define AP_FLAG    0x800        ; + 4 * core: 1 started, 2 finished ok, 0xBAD failed
%define ICR0       0xFEE00300
%define ICR1       0xFEE00310
%define ROUNDS     20000
%define IPI_COUNT  0x820
%define AP_IDT     0x21000

; spins ROUNDS times with the values of core %1 in all registers; jumps to %2 when a value changed
%macro SPIN 2
    mov eax, 0x11110000 + %1
    mov ebx, 0x22220000 + %1
    mov ecx, ROUNDS
    mov edx, 0x44440000 + %1
    mov esi, 0x55550000 + %1
    mov edi, 0x66660000 + %1
    mov ebp, 0x77770000 + %1
    movd xmm1, eax
    pshufd xmm1, xmm1, 0
    fld1
%%loop:
    cmp eax, 0x11110000 + %1
    jne %%f1
    cmp ebx, 0x22220000 + %1
    jne %%f2
    cmp edx, 0x44440000 + %1
    jne %%f3
    cmp esi, 0x55550000 + %1
    jne %%f4
    cmp edi, 0x66660000 + %1
    jne %%f5
    cmp ebp, 0x77770000 + %1
    jne %%f6
    movd edx, xmm1
    cmp edx, 0x11110000 + %1
    jne %%f7
    mov edx, 0x44440000 + %1
    fld1
    faddp st1, st0              ; st0 = 2
    fstp st0
    fld1
    fstp st0
    stc
    sbb esi, 0                  ; esi - 1 (carry chain)
    add esi, 1
    cmp esp, 0x68000 + %1 * 0x10000 - 0x100
    jne %%f8
    test ecx, 63
    jnz %%noipi
    mov dword [ICR0], 0x000C0040 | ((%1 & 1) << 19) | ((%1 & 2) << 17)   ; all but self / all including self
%%noipi:
    dec ecx
    jnz %%loop
    fstp st0
    jmp %%done
%%f1:
    mov dword [0x7C0], 1
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%f2:
    mov dword [0x7C0], 2
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%f3:
    mov dword [0x7C0], 3
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%f4:
    mov dword [0x7C0], 4
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%f5:
    mov dword [0x7C0], 5
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%f6:
    mov dword [0x7C0], 6
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%f7:
    mov dword [0x7C0], 7
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%f8:
    pushfd
    pop dword [0x7E0]
    mov dword [0x7C0], 8
    mov [0x7C4], eax
    mov [0x7C8], ebx
    mov [0x7CC], edx
    mov [0x7D0], esi
    mov [0x7D4], edi
    mov [0x7D8], ebp
    mov [0x7DC], esp
    jmp %2
%%done:
%endmacro

test_main:
    mov dword [AP_FLAG + 4], 0
    mov dword [AP_FLAG + 8], 0
    ; interrupt handler for vector 0x40 on every core (the boot core's IDT is the harness one)
    mov edi, IDT_BASE + 0x40 * 8
    mov eax, ipi_handler
    mov word [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov word [edi + 6], ax
    mov word [idt_ptr], 0x41 * 8 - 1
    mov dword [idt_ptr + 2], IDT_BASE
    lidt [idt_ptr]
    mov dword [0xFEE000F0], 0x1FF
    mov al, 0xFF                                ; no legacy interrupts (the timer would hit the harness exception vectors)
    out 0x21, al
    out 0xA1, al
    mov esi, ap_src
    mov edi, 0x9000
    mov ecx, ap_end - ap_entry
    cld
    rep movsb
    mov dword [ICR1], 1 << 24
    mov dword [ICR0], 0x0000C500
    mov dword [ICR0], 0x00008500
    mov dword [ICR0], 0x00000609
    mov dword [ICR1], 2 << 24
    mov dword [ICR0], 0x0000C500
    mov dword [ICR0], 0x00008500
    mov dword [ICR0], 0x00000609
    mov ecx, 20000000
.wait_start:
    cmp dword [AP_FLAG + 4], 1
    jb .w1
    cmp dword [AP_FLAG + 8], 1
    jae .started
.w1:
    dec ecx
    jnz .wait_start
    FAIL16 1
.started:
    mov esp, 0x68000 - 0x100
    sti
    SPIN 0, .bad
    mov esp, 0x70000 - 0x400
    cli
    mov ecx, 50000000
.wait_end:
    mov eax, [AP_FLAG + 4]
    cmp eax, 0xBAD
    je .bad
    mov edx, [AP_FLAG + 8]
    cmp edx, 0xBAD
    je .bad
    cmp eax, 2
    jne .w2
    cmp edx, 2
    je .done
.w2:
    dec ecx
    jnz .wait_end
    FAIL16 2
.done:
    cmp dword [IPI_COUNT], 20
    jb .noirq
    PASS
.noirq:
    FAIL16 4
.bad:
    FAIL16 3

ipi_handler:
    lock inc dword [IPI_COUNT]
    mov dword [0xFEE000B0], 0
    iretd

ap_src:
section .ap vstart=0x9000 follows=.text align=1
bits 16
ap_entry:
    cli
    xor ax, ax
    mov ds, ax
    lgdt [gdt_ptr]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp dword 0x08:ap32
bits 32
ap32:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov dword [0xFEE000F0], 0x1FF
    lidt [idt_ptr]
    mov eax, [0xFEE00020]
    shr eax, 24
    cmp eax, 1
    je ap1
    jmp ap2
%macro AP 1
ap%1:
    mov esp, 0x68000 + %1 * 0x10000 - 0x100
    mov dword [AP_FLAG + %1 * 4], 1
    sti
.go:
    cmp dword [AP_FLAG + 4], 1
    jb .go
    cmp dword [AP_FLAG + 8], 1
    jb .go
    SPIN %1, .bad
    mov dword [AP_FLAG + %1 * 4], 2
.halt:
    sti                         ; (a hlt with interrupts off ends the test run)
    hlt
    jmp .halt
.bad:
    mov dword [AP_FLAG + %1 * 4], 0xBAD
    jmp .halt
%endmacro
    AP 1
    AP 2
ap_end:
