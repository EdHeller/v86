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
PT4K        equ 0x30000         ; a page table of 4 KiB pages for 0x800000..0xBFFFFF (the 4k benchmarks)
FOURK       equ 0x800000
PT4K_MANY   equ 0x40000         ; 16 page tables of 4 KiB pages for WIDE..WIDE+64 MiB

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
    mov eax, 0x87               ; present, writable, user, 4 MiB
    mov ecx, 1024
.pd:
    mov [edi], eax
    add eax, 0x400000
    add edi, 4
    loop .pd
    mov eax, cr4
    or eax, (1 << 4) | (1 << 9) | (1 << 10)     ; PSE, OSFXSR, OSXMMEXCPT
    mov cr4, eax
    ; 0x800000..0xBFFFFF in 4 KiB pages (PT4K), and WIDE..WIDE+64 MiB too (PT4K_MANY, for tlb4k)
    mov edi, PT4K
    mov eax, FOURK | 3
    mov ecx, 1024
.pt:
    mov [edi], eax
    add eax, 4096
    add edi, 4
    loop .pt
    mov dword [PDIR + (FOURK >> 22) * 4], PT4K | 3
    mov edi, PT4K_MANY
    mov eax, WIDE | 3
    mov ecx, 16 * 1024
.pt2:
    mov [edi], eax
    add eax, 4096
    add edi, 4
    loop .pt2
    mov edi, PDIR + (WIDE >> 22) * 4
    mov eax, PT4K_MANY | 3
    mov ecx, 16
.pd2:
    mov [edi], eax
    add eax, 4096
    add edi, 4
    loop .pd2
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
    mov eax, pf_handler
    mov edi, IDT + 14 * 8
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

bits 16
code16_loop:
    mov ecx, 40000000
.l: add ax, bx
    xor ax, 0x5A5A
    mov [0x6000], ax
    add bx, [0x6000]
    shl ax, 1
    dec ecx
    jnz .l
    jmp dword 0x08:code16_back
bits 32

; #PF: the page table entry of the faulting page becomes present again
pf_handler:
    push eax
    mov eax, cr2
    shr eax, 12
    and eax, 1023
    or dword [PT4K + eax * 4], 1
    pop eax
    add esp, 4                  ; the error code
    iretd

align 4
benchmarks:
    dd b_alu, b_flags, b_memory, b_stack, b_call_near, b_call_far, b_indirect, b_string, b_muldiv, b_sse, b_x87
    dd b_pagewalk, b_smc, b_port, b_interrupt, b_pit
    dd b_call_deep, b_setcc, b_bitops, b_unaligned, b_rep_small, b_scalar_fp, b_lock, b_rdtsc, b_pagefault
    dd b_tlb4k, b_code16, b_segment
    dd b_sysenter, b_cli_sti, b_spinlock, b_cpuid, b_memcpy_bytes

; 0 alu: dependent register arithmetic, no memory
b_alu:
    mov ecx, 160000000
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
    mov ecx, 160000000
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
    mov ecx, 80000000
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
    mov ecx, 80000000
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
    mov ecx, 80000000
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
    mov ecx, 40000000
.l: call far_fn
    call far_fn
    dec ecx
    jnz .l
    ret

; 6 indirect: calls through a register and jumps through a table
b_indirect:
    mov ecx, 40000000
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
    mov edx, 8000000
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
    mov ecx, 40000000
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
    mov ecx, 40000000
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
    mov ecx, 12000000
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
    mov edx, 4000
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
    mov ecx, 24000000
.l: mov byte [.target + 1], cl
.target:
    mov al, 0
    add ebx, eax
    dec ecx
    jnz .l
    ret

; 13 port: port I/O to an unused port (a call out of WebAssembly each time)
b_port:
    mov ecx, 4000000
    mov dx, 0x1E00
.l: in al, dx
    out dx, al
    dec ecx
    jnz .l
    ret

; 14 interrupt: software interrupts and iret
b_interrupt:
    mov ecx, 8000000
.l: int 0x80
    dec ecx
    jnz .l
    ret

; 15 pit: reading the PIT's counter (a JavaScript device)
b_pit:
    mov ecx, 8000000
.l: mov al, 0
    out 0x43, al
    in al, 0x40
    in al, 0x40
    dec ecx
    jnz .l
    ret


; 16 call_deep: recursion 32 deep and back
b_call_deep:
    mov ecx, 2000000
.l: mov edx, 32
    call .rec
    dec ecx
    jnz .l
    ret
.rec:
    dec edx
    jz .r
    call .rec
.r: ret

; 17 setcc: setcc, cmovcc, adc, sbb (flags consumed many ways)
b_setcc:
    mov ecx, 40000000
    xor eax, eax
    xor ebx, ebx
.l: cmp ecx, 0x8000000
    setb al
    cmovg ebx, ecx
    adc ebx, eax
    sbb edx, eax
    cmp edx, ebx
    setz ah
    dec ecx
    jnz .l
    ret

; 18 bitops: bt, bts, bsf, bsr, shld, rol
b_bitops:
    mov ecx, 30000000
    mov eax, 0x12345678
.l: bt eax, ecx
    bts ebx, ecx
    bsf edx, eax
    bsr esi, eax
    shld eax, ebx, 3
    rol eax, 5
    or eax, 1
    dec ecx
    jnz .l
    ret

; 19 unaligned: loads and stores that cross a 4 KiB page boundary
b_unaligned:
    mov ecx, 5000000
    mov esi, DATA + 4096 - 2
.l: mov eax, [esi]
    add eax, ecx
    mov [esi], eax
    mov edx, [esi + 4096]
    add [esi + 4096], edx
    dec ecx
    jnz .l
    ret

; 20 rep_small: short rep movsb and rep stosd (16 bytes)
b_rep_small:
    mov edx, 8000000
.l: mov esi, DATA
    mov edi, DATA + 64
    mov ecx, 16
    rep movsb
    mov edi, DATA + 128
    mov ecx, 4
    rep stosd
    dec edx
    jnz .l
    ret

; 21 scalar_fp: scalar SSE single and double, conversions
b_scalar_fp:
    mov ecx, 20000000
    mov eax, 3
    cvtsi2sd xmm0, eax
    cvtsi2ss xmm1, eax
.l: addsd xmm0, xmm0
    mulsd xmm0, xmm0
    cvtsi2sd xmm2, ecx
    addsd xmm0, xmm2
    cvttsd2si eax, xmm0
    addss xmm1, xmm1
    mulss xmm1, xmm1
    xorps xmm0, xmm0
    xorps xmm1, xmm1
    dec ecx
    jnz .l
    ret

; 22 lock: locked read-modify-write, xchg with memory, cmpxchg
b_lock:
    mov ecx, 60000000
    mov esi, DATA
.l: lock add dword [esi], 1
    mov eax, ecx
    xchg [esi + 4], eax
    mov eax, [esi + 8]
    lock cmpxchg [esi + 8], ecx
    dec ecx
    jnz .l
    ret

; 23 rdtsc: the time stamp counter
b_rdtsc:
    mov ecx, 20000000
.l: rdtsc
    dec ecx
    jnz .l
    ret

; 24 pagefault: a #PF on a page that is not present, the handler maps it, the access repeats
b_pagefault:
    mov ecx, 3000000
.l: and dword [PT4K + 5 * 4], ~1
    invlpg [FOURK + 5 * 4096]
    mov eax, [FOURK + 5 * 4096]
    dec ecx
    jnz .l
    ret

; 25 tlb4k: one load per 4 KiB page over 64 MiB of 4 KiB pages (two level walks), CR3 reloads
b_tlb4k:
    mov edx, 400
.p: mov esi, WIDE
    mov ecx, 16384
.l: mov eax, [esi]
    add esi, 4096
    dec ecx
    jnz .l
    mov eax, cr3
    mov cr3, eax
    dec edx
    jnz .p
    ret

; 26 code16: a loop in a 16-bit code segment (operand and address size 16)
b_code16:
    jmp 0x28:code16_loop
code16_back:
    ret

; 27 segment: loads and stores through a segment whose base is not zero
b_segment:
    mov ax, 0x30
    mov fs, ax
    mov ecx, 80000000
.l: mov eax, [fs:0x10000]
    add eax, ecx
    mov [fs:0x10004], eax
    add [fs:0x10008], eax
    dec ecx
    jnz .l
    mov ax, 0x10
    mov fs, ax
    ret

; 28 sysenter: a user mode loop (ring 3) of memory accesses and a system call through sysenter / sysexit each time,
; as 32-bit Linux makes system calls (the vDSO's __kernel_vsyscall)
USTACK equ 0x80000
b_sysenter:
    mov ecx, 0x174              ; IA32_SYSENTER_CS
    mov eax, 0x08
    xor edx, edx
    wrmsr
    mov ecx, 0x175              ; IA32_SYSENTER_ESP
    mov eax, STACK - 0x1000
    wrmsr
    mov ecx, 0x176              ; IA32_SYSENTER_EIP
    mov eax, sys_entry
    wrmsr
    mov [saved_esp], esp
    mov edx, user_loop
    mov ecx, USTACK
    sysexit
sys_entry:
    test eax, eax
    jnz .finish
    sysexit                     ; back to edx with esp = ecx
.finish:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov esp, [saved_esp]
    ret
user_loop:
    mov ax, 0x23
    mov ds, ax
    mov es, ax
    mov esi, DATA
    mov ebx, 8000000
.l: mov eax, [esi]
    add eax, ebx
    mov [esi + 4], eax
    xor eax, eax
    mov ecx, esp
    mov edx, .ret
    sysenter
.ret:
    dec ebx
    jnz .l
    mov eax, 1
    sysenter
align 4
saved_esp: dd 0

; 29 cli_sti: interrupt flag changes and pushfd / popfd, as in kernel critical sections
b_cli_sti:
    mov ecx, 20000000
.l: pushfd
    cli
    add eax, ecx
    popfd
    sti
    cli
    dec ecx
    jnz .l
    ret

; 30 spinlock: take and release a lock (lock xchg, a test, a store, pause)
b_spinlock:
    mov ecx, 80000000
    mov esi, DATA + 0x100
.l: mov eax, 1
    xchg [esi], eax
    test eax, eax
    jnz .busy
    add dword [esi + 4], 1
    mov dword [esi], 0
.busy:
    pause
    dec ecx
    jnz .l
    ret

; 31 cpuid
b_cpuid:
    mov edi, 60000000
.l: mov eax, 1
    cpuid
    dec edi
    jnz .l
    ret

; 32 memcpy_bytes: a byte at a time copy loop, as a simple C memcpy compiles
b_memcpy_bytes:
    mov edx, 600000
.p: mov esi, DATA
    mov edi, DATA + 0x8000
    mov ecx, 256
.l: mov al, [esi]
    mov [edi], al
    inc esi
    inc edi
    dec ecx
    jnz .l
    dec edx
    jnz .p
    ret

align 8
gdt:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
    dq 0x00CFFA000000FFFF       ; 0x18: user code, 32-bit, DPL 3 (sysexit: SYSENTER_CS + 16)
    dq 0x00CFF2000000FFFF       ; 0x20: user data, DPL 3 (SYSENTER_CS + 24)
    dq 0x00009A000000FFFF       ; 0x28: 16-bit code, base 0, limit 64 KiB
    dq 0x00CF92100000FFFF       ; 0x30: data, base 1 MiB
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
