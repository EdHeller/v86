; Micro benchmarks for v86 builds (tools/v86-bench/micro/run.mjs): a flat binary at 0x7C00 that enters 32-bit
; protected mode with paging (4 MiB identity pages, as an OS has), runs the benchmark whose number the host wrote to
; 0x500, and halts with interrupts off (cpu-event-halt). The host times from start to halt; each benchmark is one
; kind of work, so a change shows up where it acts.
;
;   nasm -f bin -o micro.bin micro.asm

bits 16
org 0x7C00

BENCH       equ 0x500           ; the benchmark to run (written by the host)
PDIR        equ 0x10000         ; page directory: 1024 4 MiB pages, identity
IDT         equ 0x20000
STACK       equ 0x90000
DATA        equ 0x400000        ; 4 MiB of data for the memory benchmarks
WIDE        equ 0x1000000       ; 16 MiB, for the page walk benchmark

start:
    cli
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7000
    mov al, 0xFF                ; PICs masked
    out 0x21, al
    out 0xA1, al
    lgdt [gdt_ptr]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:pm

bits 32
pm:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, STACK
    ; identity paging with 4 MiB pages
    mov edi, PDIR
    mov eax, 0x83               ; present, writable, 4 MiB
    mov ecx, 1024
.pd:
    mov [edi], eax
    add eax, 0x400000
    add edi, 4
    loop .pd
    mov eax, cr4
    or eax, (1 << 4) | (1 << 9) | (1 << 10)     ; PSE, OSFXSR, OSXMMEXCPT
    mov cr4, eax
    mov eax, PDIR
    mov cr3, eax
    mov eax, cr0
    or eax, (1 << 31) | (1 << 1)                ; PG, MP
    and eax, ~(1 << 2)                          ; EM off
    mov cr0, eax
    fninit
    ; an IDT whose vector 0x80 returns at once (the interrupt benchmark)
    mov eax, int_handler
    mov edi, IDT + 0x80 * 8
    mov [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov [edi + 6], ax
    lidt [idt_ptr]
    movzx eax, byte [BENCH]
    call [benchmarks + eax * 4]
    cli
.halt:
    hlt
    jmp .halt

int_handler:
    iretd

align 4
benchmarks:
    dd b_alu, b_flags, b_memory, b_stack, b_call_near, b_call_far, b_indirect, b_string, b_muldiv, b_sse, b_x87
    dd b_pagewalk, b_smc, b_port, b_interrupt, b_pit

; 0 alu: dependent register arithmetic, no memory
b_alu:
    mov ecx, 40000000
    xor eax, eax
    mov ebx, 3
.l: add eax, ebx
    xor eax, 0x5A5A
    shl eax, 1
    sub eax, ecx
    or eax, ebx
    and eax, 0xFFFF
    dec ecx
    jnz .l
    ret

; 1 flags: compares and conditional jumps, taken and not taken
b_flags:
    mov ecx, 40000000
    xor eax, eax
.l: cmp ecx, 1000
    jb .a
    inc eax
.a: test ecx, 1
    jz .b
    add eax, 2
.b: cmp eax, ecx
    jg .c
    sub eax, 1
.c: dec ecx
    jnz .l
    ret

; 2 memory: loads and stores in one page
b_memory:
    mov ecx, 20000000
    mov esi, DATA
.l: mov eax, [esi]
    add eax, [esi + 4]
    mov [esi + 8], eax
    mov ebx, [esi + 12]
    add [esi + 16], ebx
    mov [esi + 20], ecx
    dec ecx
    jnz .l
    ret

; 3 stack: push and pop
b_stack:
    mov ecx, 20000000
.l: push eax
    push ebx
    push ecx
    push edx
    pop edx
    pop ecx
    pop ebx
    pop eax
    dec ecx
    jnz .l
    ret

; 4 call_near: call and ret within the page
b_call_near:
    mov ecx, 20000000
.l: call near_fn
    call near_fn
    dec ecx
    jnz .l
    ret
near_fn:
    add eax, 1
    ret

; 5 call_far: call and ret to a function on another page (compiled code leaves its module)
b_call_far:
    mov ecx, 10000000
.l: call far_fn
    call far_fn
    dec ecx
    jnz .l
    ret

; 6 indirect: calls through a register and jumps through a table
b_indirect:
    mov ecx, 10000000
    mov ebx, near_fn
.l: call ebx
    mov eax, ecx
    and eax, 1
    jmp [jt + eax * 4]
.j0: add edx, 1
    jmp .n
.j1: sub edx, 1
.n: dec ecx
    jnz .l
    ret
align 4
jt: dd b_indirect.j0, b_indirect.j1

; 7 string: rep movsd and rep stosb of 4 KiB
b_string:
    mov edx, 2000000
.l: mov esi, DATA
    mov edi, DATA + 0x10000
    mov ecx, 1024
    rep movsd
    mov edi, DATA + 0x20000
    mov ecx, 4096
    xor eax, eax
    rep stosb
    dec edx
    jnz .l
    ret

; 8 muldiv: multiply and divide
b_muldiv:
    mov ecx, 10000000
    mov ebx, 7
.l: mov eax, ecx
    imul eax, eax, 13
    xor edx, edx
    div ebx
    imul edx, eax
    dec ecx
    jnz .l
    ret

; 9 sse: packed integer and float arithmetic with loads
b_sse:
    mov ecx, 10000000
    mov esi, DATA
    pxor xmm0, xmm0
    xorps xmm2, xmm2
.l: movdqa xmm1, [esi]
    paddd xmm0, xmm1
    pmullw xmm1, xmm0
    pxor xmm0, xmm1
    movaps xmm3, [esi + 16]
    addps xmm2, xmm3
    mulps xmm2, xmm3
    movdqa [esi + 32], xmm0
    dec ecx
    jnz .l
    ret

; 10 x87: floating point arithmetic
b_x87:
    mov ecx, 3000000
    fld1
    fld1
.l: fadd st0, st1
    fmul st0, st1
    fsub st0, st1
    fdiv st0, st1
    dec ecx
    jnz .l
    fstp st0
    fstp st0
    ret

; 11 pagewalk: one load per 4 KiB page over 16 MiB with paging, and a CR3 reload (TLB flush) per pass
b_pagewalk:
    mov edx, 1000
.p: mov esi, WIDE
    mov ecx, 4096
.l: mov eax, [esi]
    add esi, 4096
    dec ecx
    jnz .l
    mov eax, cr3
    mov cr3, eax
    dec edx
    jnz .p
    ret

; 12 smc: code that writes to its own page (invalidates its compiled code each time)
b_smc:
    mov ecx, 6000000
.l: mov byte [.target + 1], cl
.target:
    mov al, 0
    add ebx, eax
    dec ecx
    jnz .l
    ret

; 13 port: port I/O to an unused port (a call out of WebAssembly each time)
b_port:
    mov ecx, 1000000
    mov dx, 0x1E00
.l: in al, dx
    out dx, al
    dec ecx
    jnz .l
    ret

; 14 interrupt: software interrupts and iret
b_interrupt:
    mov ecx, 2000000
.l: int 0x80
    dec ecx
    jnz .l
    ret

; 15 pit: reading the PIT's counter (a JavaScript device)
b_pit:
    mov ecx, 2000000
.l: mov al, 0
    out 0x43, al
    in al, 0x40
    in al, 0x40
    dec ecx
    jnz .l
    ret

align 8
gdt:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt
idt_ptr:
    dw 0x81 * 8 - 1
    dd IDT

; the function of call_far, on a page of its own
align 4096
far_fn:
    add eax, 1
    ret
