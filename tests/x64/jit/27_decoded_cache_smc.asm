; EXPECT: pass
; DESC: 32-bit interpreter: the cache of decoded instructions (cpu/decoded.rs) sees writes to cached code: an immediate rewritten from another page between runs, and an instruction rewritten by the instruction before it in the same run; each function runs often enough to be decoded, too little to be compiled
%include "inc/pm32.inc"

test_main:
    xor ebx, ebx
    mov ecx, 50
.a: call fn1                        ; ebx += 3, 50 times
    dec ecx
    jnz .a
    CHECK16 1, ebx, 150
    mov dword [fn1_imm + 1], 5      ; mov eax, 3 -> mov eax, 5
    mov ecx, 50
.b: call fn1
    dec ecx
    jnz .b
    CHECK16 2, ebx, 400
    xor edx, edx
    mov ecx, 50
.c: call fn2                        ; edx += ecx: fn2 writes ecx into its next instruction
    dec ecx
    jnz .c
    CHECK16 3, edx, 1275
    PASS

align 4096
fn1:
fn1_imm:
    mov eax, 3
    add ebx, eax
    ret

align 4096
fn2:
    mov [fn2_imm + 1], ecx
fn2_imm:
    mov eax, 0
    add edx, eax
    ret
