; Common test harness. A test is a flat binary loaded at 0x7C00 and entered in
; 16-bit real mode (cs=0, ip=0x7C00). It reports through a result block in low
; memory, then halts:
;
;   0x600  dword  status: 0 = still running, 'PASS' or 'FAIL'
;   0x604  dword  id of the failing check (FAIL only)
;   0x608  qword  actual value   (FAIL only, from CHECK_* macros)
;   0x610  qword  expected value (FAIL only, from CHECK_* macros)
;
; Test header comments understood by run.js:
;   ; EXPECT: pass | xfail      (default pass; xfail = known to fail today)
;   ; ISSUE: copy/v86#1234      (optional, for reporting)
;   ; DESC: one line description
;
; Tests that only contain unprivileged instructions can also be built with
; -DNATIVE and run on a real x86-64 Linux host to validate the expected
; values (see run.js --native). These tests use `%include "inc/user.inc"`.

%define RESULT       0x600
%define RESULT_ID    0x604
%define RESULT_ACT   0x608
%define RESULT_EXP   0x610
%define STATUS_PASS  0x53534150
%define STATUS_FAIL  0x4C494146

%macro PASS_REAL 0
    mov dword [RESULT], STATUS_PASS
    hlt
    jmp $-1
%endmacro

; 16-bit helpers ------------------------------------------------------------

%macro FAIL16 1
    mov dword [RESULT_ID], %1
    mov dword [RESULT], STATUS_FAIL
    hlt
    jmp $-1
%endmacro

; compare reg/mem with imm in 16/32-bit code, fail with id on mismatch
%macro CHECK16 3     ; id, operand, imm
    cmp %2, %3
    je %%ok
    FAIL16 %1
%%ok:
%endmacro
