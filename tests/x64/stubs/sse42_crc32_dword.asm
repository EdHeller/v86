; EXPECT: xfail
; QEMU: xfail  (qemu 8.2 TCG computes the 32-bit form wrongly; expected value verified on host hardware)
; DESC: SSE4.2 CRC32 r32, r/m32 and r32, r/m8 (F2 0F 38 F0/F1)
%include "inc/real.inc"

test_main:
    xor eax, eax
    mov ecx, 0x44332211
    crc32 eax, ecx
    CHECK16 1, eax, 0x68428225
    xor eax, eax
    mov cl, 0x11
    crc32 eax, cl
    CHECK16 2, eax, 0xE235446C
    mov eax, 0x68428225
    crc32 eax, byte [data]                      ; chain: previous crc as seed
    CHECK16 3, eax, 0x1524D840
    PASS
data: db 0
