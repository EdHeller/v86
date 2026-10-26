; EXPECT: pass
; NATIVE: yes
; V86: {"jit64_threshold": 10, "sync_jit": true}
; DESC: jit64: counted loops, nested loops, forward and backward conditional jumps, calls inside loops
%include "inc/long.inc"

test_main:
    ; sum 1..2000
    xor eax, eax
    mov ecx, 2000
.sum:
    add rax, rcx
    dec ecx
    jnz .sum
    CHECK_EQ 1, rax, 2001000

    ; nested loops: count pairs (i, j) with i*j odd, 0 <= i,j < 60
    xor ebx, ebx
    xor esi, esi
.outer:
    xor edi, edi
.inner:
    mov rax, rsi
    imul rax, rdi                   ; imul: not compiled, mixes interpreter and blocks
    test al, 1
    jz .even
    inc rbx
.even:
    inc edi
    cmp edi, 60
    jb .inner
    inc esi
    cmp esi, 60
    jb .outer
    CHECK_EQ 2, rbx, 900            ; 30 odd i * 30 odd j

    ; loop with a forward exit and a signed comparison
    mov rcx, -500
    xor edx, edx
.neg:
    add rdx, rcx
    inc rcx
    cmp rcx, 100
    jge .done
    jmp .neg
.done:
    CHECK_EQ 3, rdx, -120300        ; sum of -500 .. 99
    CHECK_EQ 4, rcx, 100

    ; call / ret inside a loop, with push / pop and a stack frame
    xor r12d, r12d
    mov r13d, 300
.calls:
    mov rdi, r13
    call square_plus_one
    add r12, rax
    dec r13d
    jnz .calls
    CHECK_EQ 5, r12, 9045350        ; sum of i*i+1 for i = 1..300
    PASS

square_plus_one:
    push rbp
    mov rbp, rsp
    push rbx
    mov rbx, rdi
    imul rbx, rdi
    lea rax, [rbx + 1]
    pop rbx
    leave
    ret
