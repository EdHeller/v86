; EXPECT: xfail
; DESC: INVD and PREFETCHW execute (as cache-control / hint) instead of being treated as undefined
%include "inc/real.inc"

test_main:
    mov dword [TMP], 0x12345678
    prefetchw [TMP]                             ; 0F 0D /1, a hint with no architectural effect
    prefetchnta [TMP]
    db 0x0F, 0x0D, 0x00                         ; prefetch (3DNow!/AMD) m8
    CHECK16 1, dword [EXC_VEC], -1
    CHECK16 2, dword [TMP], 0x12345678
    invd                                        ; CPL0: valid, discards caches (memory contents are architecturally undefined afterwards, only checking no #UD)
    CHECK16 3, dword [EXC_VEC], -1
    PASS
