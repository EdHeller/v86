; EXPECT: pass
; V86: {"cpu_type": "x86", "wasm_path": "build/v86-debug.wasm"}
; DESC: the 32-bit JIT runs 66 0F 38 / 66 0F 3A (pshufb, palignr) with their 66 prefix (it lost it, so a 32-bit kernel
; DESC: hung with SSSE3/SSE4 advertised: OpenSSL's SSSE3 code computed wrong hashes); the loop runs long enough to be compiled
%include "inc/pm32.inc"

%define TMP 0x680

%macro CHECK_XMM 6          ; id, xmmN, d0, d1, d2, d3
    movups [TMP], %2
    CHECK16 (%1 << 4) | 0, dword [TMP], %3
    CHECK16 (%1 << 4) | 1, dword [TMP + 4], %4
    CHECK16 (%1 << 4) | 2, dword [TMP + 8], %5
    CHECK16 (%1 << 4) | 3, dword [TMP + 12], %6
%endmacro

test_main:
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, 1 << 1
    mov cr0, eax
    mov eax, cr4
    or eax, (1 << 9) | (1 << 10)
    mov cr4, eax
    mov ecx, 100000
.loop:
    movdqa xmm0, [bytes]
    pshufb xmm0, [reverse]          ; 66 0F 38 00 /r, memory operand
    movdqa xmm1, [reverse]
    movdqa xmm4, [bytes]
    pshufb xmm4, xmm1               ; register operand
    movdqa xmm2, [high]
    palignr xmm2, [bytes], 4        ; 66 0F 3A 0F /r ib
    dec ecx
    jnz .loop
    CHECK_XMM 1, xmm0, 0x0C0D0E0F, 0x08090A0B, 0x04050607, 0x00010203
    CHECK_XMM 2, xmm4, 0x0C0D0E0F, 0x08090A0B, 0x04050607, 0x00010203
    CHECK_XMM 3, xmm2, 0x07060504, 0x0B0A0908, 0x0F0E0D0C, 0x13121110
    PASS

align 16
bytes:   db 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D, 0x0E, 0x0F
reverse: db 0x0F, 0x0E, 0x0D, 0x0C, 0x0B, 0x0A, 0x09, 0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, 0x00
high:    db 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1A, 0x1B, 0x1C, 0x1D, 0x1E, 0x1F
