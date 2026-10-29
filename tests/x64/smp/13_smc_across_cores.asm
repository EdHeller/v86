; EXPECT: pass
; V86: {"acpi": true, "cpu_cores": 2, "cpu_workers": true, "vcpu_worker_url": "src/parallel/vcpu_worker.js", "wasm_path": "build/v86-parallel-debug.wasm"}
; DESC: a write by the boot core to code that the core in a worker has compiled (hot loop, jit64 in the worker) takes effect there: the worker retires its compiled code
%include "inc/long.inc"

%define AP_DONE    0x800
%define OBSERVED   0x810
%define STOP       0x814

test_main:
    mov rdi, 0x5000
    xor eax, eax
    mov ecx, 512
    rep stosq
    mov qword [0x3000 + 3 * 8], 0x5000 | 7
    mov eax, 0xFEE00000 | 0x97
    mov [0x5000 + 503 * 8], rax
    mov rax, cr3
    mov cr3, rax
    xor eax, eax
    mov rdi, 0x800
    mov ecx, 16
    rep stosd
    lea rsi, [rel ap_src]
    mov edi, 0x9000
    mov ecx, ap_end - ap_entry
    cld
    rep movsb
    mov ebx, 0xFEE00300
    mov dword [rbx + 0x10], 1 << 24
    mov dword [rbx], 0x0000C500
    mov dword [rbx], 0x00008500
    mov dword [rbx], 0x00000609
    ; the worker runs fn (which returns 1) in a loop and stores what it returns
    mov rcx, 4000000000
.wait1:
    cmp dword [OBSERVED], 1
    je .started
    dec rcx
    jnz .wait1
    mov eax, 1
    xor r15, r15
    call report_fail
.started:
    ; let it become hot (compiled), then change the immediate of fn: mov eax, N
    mov r8d, 2
.change:
    mov ecx, 3000000
.hot:
    dec ecx
    jnz .hot
    mov dword [0x9000 + (fn - ap_entry) + 1], r8d
    mov rcx, 4000000000
.seen:
    cmp dword [OBSERVED], r8d
    je .next
    dec rcx
    jnz .seen
    mov eax, 2
    mov r15d, [OBSERVED]
    call report_fail
.next:
    inc r8d
    cmp r8d, 8
    jb .change
    mov dword [STOP], 1
    PASS

ap_src:
section .ap vstart=0x9000 follows=.text align=1
bits 16
ap_entry:
    cli
    xor ax, ax
    mov ds, ax
    lgdt [gdt_ptr]
    mov eax, cr4
    or eax, (1 << 5) | (1 << 9) | (1 << 10)
    mov cr4, eax
    mov eax, PML4
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, (1 << 8) | 1
    wrmsr
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, (1 << 31) | (1 << 1) | 1
    mov cr0, eax
    jmp dword 0x08:ap64
bits 64
ap64:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov rsp, 0x1800000
.loop:
    call fn
    mov [OBSERVED], eax
    cmp dword [STOP], 1
    jne .loop
.halt:
    sti
    hlt
    jmp .halt
times (64 - ($ - ap_entry) % 64) % 64 db 0x90
fn:
    mov eax, 1
    ret
ap_end:
