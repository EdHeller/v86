; EXPECT: pass
; DESC: 32-bit JIT: relative jumps that cross into the next page (jnz near with 3 bytes in the next page, taken and not; jmp short with its offset in the next page) compiled at the end of their blocks; results from a model, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov ecx, 100000                 ; (enough to compile the loop)
    xor eax, eax
    xor ebx, ebx
    xor edx, edx
    call body
    CHECK16 1, eax, 0x493e0
    CHECK16 2, ebx, 0xbf08eb00
    CHECK16 3, edx, 0x5583296c
    PASS

align 4096
page0:
    times 4096 - 64 nop
body:
loop0:
    add eax, 3
    test ecx, 1
    times (4096 - 3) - ($ - page0) nop
    jnz near odd                    ; 0F 85 xx | xx xx xx: crosses into the next page
    add ebx, eax                    ; even
    jmp page1_go
odd:
    xor edx, eax
    jmp page1_go

align 4096
page1:
    times 4096 - 64 nop
page1_go:
    rol edx, 1
    times (4096 - 1) - ($ - page1) nop
    jmp short after                 ; EB | xx: crosses into the next page
after:
    add edx, ebx
    dec ecx
    jnz loop0
    ret
