; EXPECT: pass
; V86: {"acpi": true, "cpu_cores": 2, "cpu_workers": true, "vcpu_worker_url": "src/parallel/vcpu_worker.js", "wasm_path": "build/v86-parallel-debug.wasm"}
; DESC: the second core runs in a Web Worker (its own instance of v86-parallel.wasm): INIT-SIPI starts it, IPIs reach it, an IPI to a core that does not exist is dropped
%include "inc/pm32.inc"

%define AP_ID      0x800
%define AP_IRQS    0x804
%define AP_IDT     0x21000
%define ICR0       0xFEE00300
%define ICR1       0xFEE00310

%macro WAIT_FOR 3               ; dword address, value, fail id
    mov ecx, 20000000
%%l:
    cmp dword [%1], %2
    je %%ok
    dec ecx
    jnz %%l
    FAIL16 %3
%%ok:
%endmacro

test_main:
    mov eax, 1
    cpuid
    mov eax, ebx
    shr eax, 16
    and eax, 0xFF
    cmp eax, 2
    je .count
    FAIL16 1                    ; logical processor count
.count:
    shr ebx, 24
    jz .id
    FAIL16 2                    ; the boot core has initial APIC id 0
.id:
    mov eax, [0xFEE00020]
    shr eax, 24
    jz .lapic_id
    FAIL16 3
.lapic_id:
    mov dword [AP_ID], 0xFFFFFFFF
    mov dword [AP_IRQS], 0

    ; code of the second core to 0x9000
    mov esi, ap_src
    mov edi, 0x9000
    mov ecx, ap_end - ap_entry
    cld
    rep movsb
    ; its IDT: vector 0x40 -> ap_handler
    mov edi, AP_IDT + 0x40 * 8
    mov eax, ap_handler
    mov word [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov word [edi + 6], ax
    mov word [AP_IDT - 8], 0x41 * 8 - 1
    mov dword [AP_IDT - 6], AP_IDT

    ; INIT, then start-up IPI with vector 9 (0x9000) to core 1
    mov dword [ICR1], 1 << 24
    mov dword [ICR0], 0x0000C500            ; INIT, level, assert
    mov dword [ICR0], 0x00008500            ; INIT, level, de-assert
    mov dword [ICR0], 0x00000609            ; start-up, vector 9
    WAIT_FOR AP_ID, 1, 4                    ; core 1 runs and reports its APIC id

    ; fixed IPI with physical destination 1
    mov dword [ICR1], 1 << 24
    mov dword [ICR0], 0x00004040
    WAIT_FOR AP_IRQS, 1, 5

    ; broadcast to all but self
    mov dword [ICR0], 0x000C4040
    WAIT_FOR AP_IRQS, 2, 6

    ; an IPI to a core that does not exist is dropped
    mov dword [ICR1], 5 << 24
    mov dword [ICR0], 0x00004040
    mov ecx, 200000
.settle:
    dec ecx
    jnz .settle
    cmp dword [AP_IRQS], 2
    je .done
    FAIL16 7
.done:
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
    lidt [AP_IDT - 8]
    mov dword [0xFEE000F0], 0x1FF           ; software enable the local APIC
    mov eax, [0xFEE00020]
    shr eax, 24
    mov [AP_ID], eax
    sti
.idle:
    hlt
    jmp .idle
ap_handler:
    inc dword [AP_IRQS]
    mov dword [0xFEE000B0], 0               ; EOI
    iretd
ap_end:
