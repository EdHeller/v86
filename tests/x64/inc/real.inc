; 16-bit real mode test environment: segments zeroed, stack at 0x7000,
; IVT vectors 0-31 point at stubs that record the exception and resume.
;
;     %include "inc/real.inc"
;     test_main:
;         TRAP_BEGIN
;             int 5
;         TRAP_END 1, 5        ; check id, expected vector
;         PASS_REAL
;
; Recorded state (dwords): EXC_VEC (-1 if none), EXC_RIP (ip of the saved
; frame), EXC_COUNT. Unexpected exceptions (no TRAP_BEGIN active) fail the test
; with id 0xDEAD0000 | vector.

bits 16
org 0x7C00

%include "inc/harness.inc"

%define EXC_VEC     0x620
%define EXC_RIP     0x630
%define EXC_COUNT   0x638
%define EXC_RESUME  0x640
%define TMP         0x680       ; 16 bytes scratch for CHECK_XMM

start16:
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000
    cld
    xor di, di
    mov bx, rm_isr_table
    mov cx, 32
.ivt:
    mov [di], bx
    mov word [di + 2], 0
    add di, 4
    add bx, 8
    loop .ivt
    mov dword [EXC_COUNT], 0
    mov dword [EXC_RESUME], 0
    mov dword [EXC_VEC], -1
    jmp test_main

align 8
rm_isr_table:
%assign v 0
%rep 32
align 8
    push byte v
    jmp rm_exc_common
%assign v v+1
%endrep

rm_exc_common:
    push bp
    mov bp, sp                  ; [bp+2]=vector [bp+4]=ip [bp+6]=cs [bp+8]=flags
    push eax
    movzx eax, word [bp + 2]
    mov [EXC_VEC], eax
    movzx eax, word [bp + 4]
    mov [EXC_RIP], eax
    inc dword [EXC_COUNT]
    mov ax, [EXC_RESUME]
    test ax, ax
    jz .unexpected
    mov [bp + 4], ax
    pop eax
    pop bp
    add sp, 2
    iret
.unexpected:
    mov eax, [EXC_VEC]
    or eax, 0xDEAD0000
    mov [RESULT_ID], eax
    mov dword [RESULT], STATUS_FAIL
    hlt
    jmp $-1

; Run code expected to raise exception `vec` (resumes after TRAP_END):
%macro TRAP_BEGIN 0
    %push trap
    mov word [EXC_RESUME], %$after
    mov dword [EXC_VEC], -1
%endmacro
%macro TRAP_END 2
%$after:
    mov word [EXC_RESUME], 0
    CHECK16 %1, dword [EXC_VEC], %2
    %pop
%endmacro

; Enable SSE/SSE2+ state handling (CR0.EM=0, CR0.MP=1, CR4.OSFXSR|OSXMMEXCPT)
%macro SSE_ON 0
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, 1 << 1
    mov cr0, eax
    mov eax, cr4
    or eax, (1 << 9) | (1 << 10)
    mov cr4, eax
%endmacro

; Compare the four dwords of an xmm register
%macro CHECK_XMM 6          ; id, xmmN, d0, d1, d2, d3
    movups [TMP], %2
    CHECK16 (%1 << 4) | 0, dword [TMP], %3
    CHECK16 (%1 << 4) | 1, dword [TMP + 4], %4
    CHECK16 (%1 << 4) | 2, dword [TMP + 8], %5
    CHECK16 (%1 << 4) | 3, dword [TMP + 12], %6
%endmacro

%macro PASS 0
    PASS_REAL
%endmacro
