; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: TempleOS scheduler block (bt/test/cmp+jg on memory, rip relative load, call) over many task states
%include "inc/long.inc"

%define EXPECTED 0xF8BDB28EC179130B
%macro MIX 1
    xor r10, %1
    rol r10, 7
    add r10, r9
%endmacro

test_main:
    xor r10d, r10d
    mov r9d, 0x2545F491
    xor r11d, r11d
.iter:
    lea rsi, [rel task]
    ; [task+0x18]: flag words, [task+0x10]: wake time, jiffies: current time
    mov eax, r11d
    and eax, 7
    lea rbx, [rel flagtab]
    mov rax, [rbx + rax*8]
    mov [rsi + 0x18], rax
    mov eax, r11d
    shr eax, 3
    and eax, 7
    lea rbx, [rel timetab]
    mov rax, [rbx + rax*8]
    mov [rsi + 0x10], rax
    mov eax, r11d
    shr eax, 6
    and eax, 3
    lea rbx, [rel nowtab]
    mov rax, [rbx + rax*8]
    mov [rel jiffies], rax
    xor edx, edx
    mov eax, 0x1234
    ; the scheduler's loop body
    bt qword [rsi + 0x18], 1
    jc .p1
    test qword [rsi + 0x18], 0x204
    jnz .p2
    mov rax, [rel jiffies]
    cmp [rsi + 0x10], rax
    jg .p2
    mov rax, rsi
    call fn
    mov edx, 3
    jmp .done
.p1:
    mov edx, 1
    jmp .done
.p2:
    mov edx, 2
.done:
    MIX rdx
    cmp rax, rsi
    jne .nz
    xor eax, eax
.nz:
    MIX rax
    inc r11d
    cmp r11d, 200
    jb .iter

%ifdef DUMP
    lea rdi, [rel hexbuf]
    mov rax, r10
    mov ecx, 16
.hex:
    rol rax, 4
    mov edx, eax
    and edx, 15
    add dl, '0'
    cmp dl, '9'
    jbe .digit
    add dl, 7
.digit:
    mov [rdi], dl
    inc rdi
    dec ecx
    jnz .hex
    mov byte [rdi], 10
    mov eax, 1
    mov edi, 1
    lea rsi, [rel hexbuf]
    mov edx, 17
    syscall
    mov eax, 60
    xor edi, edi
    syscall
%endif
    CHECK_EQ 1, r10, EXPECTED
    PASS

fn:
    push rax
    add rax, 0x10
    pop rax
    ret

section .data
align 4096
task: times 32 dq 0
jiffies: dq 0
flagtab: dq 0xb00000008, 2, 0x204, 4, 0, 0x200, 0xFFFFFFFFFFFFFFF0, 0x100000000
timetab: dq 0, 5, 0x12266, 0x12267, 0xFFFFFFFFFFFFFFFF, 0x8000000000000000, 0x7FFFFFFFFFFFFFFF, 0x12265
nowtab: dq 0x12266, 0, 0xFFFFFFFFFFFFFFFF, 0x7FFFFFFFFFFFFFFF
hexbuf: times 20 db 0
