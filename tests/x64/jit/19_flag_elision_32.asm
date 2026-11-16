; EXPECT: pass
; DESC: 32-bit JIT: arithmetic whose flags the next flag-setting instruction overwrites (their stores left out), next to arithmetic whose flags are read; results and the flags that are seen, interpreted and compiled
%include "inc/pm32.inc"

test_main:
    mov ecx, 100000                 ; (enough to compile the loop)
    xor eax, eax
    mov ebx, 1
    xor edx, edx
    mov esi, 7
.loop:
    add eax, ebx                    ; flags dead: sub follows
    mov edi, eax
    sub esi, 3                      ; dead: xor follows
    xor edx, eax                    ; dead: cmp follows (lea in between)
    lea ebx, [ebx + eax * 2 + 1]
    cmp ebx, 0x40000000             ; read by jb
    jb .noclip
    and ebx, 0xFFFF                 ; dead: test follows
    test eax, eax                   ; dead: or follows
    or ebx, 1                       ; read at the end of the block
.noclip:
    add edx, esi                    ; read: dec keeps the carry
    dec ecx
    jnz .loop
    CHECK16 1, eax, 0x6a6aa09c
    CHECK16 2, ebx, 0x5991
    CHECK16 3, edx, 0xd7069872
    CHECK16 4, esi, 0xfffb6c27
    CHECK16 5, edi, 0x6a6aa09c

    ; dead flags across loads, stores, push and pop
    mov ecx, 100000
    mov eax, 3
    mov ebx, 5
    mov dword [scratch], 7
.mem:
    push ecx
    add eax, ebx                    ; dead: sub follows, with memory accesses in between
    mov [scratch], eax
    push eax
    pop edx
    mov ecx, [scratch]
    sub ebx, ecx                    ; dead: xor follows
    mov [scratch + 4], ebx
    xor ebx, 0x5A5A
    pop ecx
    dec ecx
    jnz .mem
    CHECK16 7, eax, 0xfd7f2b74
    CHECK16 8, ebx, 0xfff2c7cd
    CHECK16 9, edx, 0xfd7f2b74

    ; the flags that are seen are those of the last writer
    mov ecx, 100000
.flags:
    mov eax, 5
    cmp eax, 7                      ; dead: add follows
    mov ebx, 5
    add ebx, ebx                    ; 10: only the parity flag
    pushfd
    pop edx
    and edx, 0x8D5
    cmp edx, 0x04
    jne .bad
    mov eax, 0xFFFFFFFF
    add eax, 1                      ; carry, zero, parity, adjust: read by adc
    adc edx, 0
    cmp edx, 0x05
    jne .bad
    sub eax, eax                    ; dead: cmp follows
    cmp eax, 1                      ; carry, sign, parity, adjust: read by sbb
    sbb edx, 0
    cmp edx, 0x04
    jne .bad
    dec ecx
    jnz .flags
    PASS
.bad:
    FAIL16 6

align 4
scratch: dd 0, 0
