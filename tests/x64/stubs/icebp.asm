; EXPECT: xfail
; QEMU: xfail  (qemu does not deliver #DB for ICEBP / does not limit-check far offsets in real mode)
; DESC: INT1 / ICEBP (0xF1) raises #DB (vector 1)
%include "inc/real.inc"

test_main:
    TRAP_BEGIN
        db 0xF1
    TRAP_END 1, 1
    PASS
