; EXPECT: pass
; V86: {"acpi": true, "cpu_cores": 2, "cpu_workers": true, "vcpu_worker_url": "src/parallel/vcpu_worker.js", "wasm_path": "build/v86-parallel-debug.wasm"}
; DESC: LOCKed read-modify-write instructions are atomic between a core in a worker and the boot core: lock inc, lock xadd, lock cmpxchg (a spin lock around a plain increment), lock bts, xchg
%include "inc/pm32.inc"

%define AP_DONE    0x800
%define COUNT_INC  0x810
%define COUNT_ADD  0x814
%define COUNT_LOCKED 0x818        ; incremented without lock, inside the spin lock
%define SPIN_LOCK  0x81C
%define COUNT_BTS  0x820
%define COUNT_XCHG 0x830
%define ICR0       0xFEE00300
%define ICR1       0xFEE00310
%define ROUNDS     30000

%macro WORK 0
    mov ecx, ROUNDS
%%loop:
    lock inc dword [COUNT_INC]
    mov eax, 3
    lock xadd [COUNT_ADD], eax
    ; spin lock: lock cmpxchg 0 -> 1, plain increment, release with a plain store
%%acquire:
    xor eax, eax
    mov edx, 1
    lock cmpxchg [SPIN_LOCK], edx
    jnz %%acquire
    mov eax, [COUNT_LOCKED]
    inc eax
    mov [COUNT_LOCKED], eax
    mov dword [SPIN_LOCK], 0
    ; a bit lock with lock bts, released with a plain store (the way SeaBIOS does it)
%%bts:
    lock bts dword [COUNT_BTS], 0
    jc %%bts
    mov eax, [COUNT_BTS + 4]
    inc eax
    mov [COUNT_BTS + 4], eax
    mov dword [COUNT_BTS], 0
    ; xchg with memory is locked implicitly: a ticket
%%xchg:
    mov eax, 1
    xchg [COUNT_XCHG], eax
    test eax, eax
    jnz %%xchg
    mov eax, [COUNT_XCHG + 4]
    inc eax
    mov [COUNT_XCHG + 4], eax
    mov dword [COUNT_XCHG], 0
    dec ecx
    jnz %%loop
%endmacro

test_main:
    xor eax, eax
    mov edi, 0x800
    mov ecx, 16
    rep stosd
    mov esi, ap_src
    mov edi, 0x9000
    mov ecx, ap_end - ap_entry
    cld
    rep movsb
    mov dword [ICR1], 1 << 24
    mov dword [ICR0], 0x0000C500
    mov dword [ICR0], 0x00008500
    mov dword [ICR0], 0x00000609
    WORK
    mov ecx, 400000000
.wait:
    cmp dword [AP_DONE], 1
    je .done
    dec ecx
    jnz .wait
    FAIL16 1
.done:
    cmp dword [COUNT_INC], 2 * ROUNDS
    je .c1
    FAIL16 2
.c1:
    cmp dword [COUNT_ADD], 6 * ROUNDS
    je .c2
    FAIL16 3
.c2:
    cmp dword [COUNT_LOCKED], 2 * ROUNDS
    je .c3
    FAIL16 4
.c3:
    cmp dword [COUNT_BTS + 4], 2 * ROUNDS
    je .c4
    FAIL16 5
.c4:
    cmp dword [COUNT_XCHG + 4], 2 * ROUNDS
    je .c5
    FAIL16 6
.c5:
    PASS

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
    mov esp, 0x68000
    WORK
    mov dword [AP_DONE], 1
.halt:
    sti
    hlt
    jmp .halt
ap_end:
