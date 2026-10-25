; EXPECT: pass
; ISSUE: copy/v86#103, copy/v86#161
; DESC: EFLAGS.TF single stepping: a #DB (int 1) trap is taken after each instruction
%include "inc/real.inc"

test_main:
    mov word [1 * 4], dbg_handler
    mov word [1 * 4 + 2], 0
    mov word [dbg_count], 0
    ; setting TF with popf: the trap fires after the following instruction
    pushf
    pop ax
    or ax, 0x100
    push ax
    popf
    nop
site1:
    nop
    nop
    CHECK16 1, word [dbg_count], 1
    CHECK16 2, word [dbg_ip], site1
    ; the trap frame saved TF=1 (the handler cleared it for the iret)
    CHECK16 3, word [dbg_flags_seen], 0x100
    PASS

dbg_handler:
    push bp
    mov bp, sp
    push ax
    mov ax, [bp + 6]
    mov [dbg_flags_seen], ax
    and ax, 0x100
    mov [dbg_flags_seen], ax
    and word [bp + 6], ~0x100                   ; stop stepping after the first trap
    mov ax, [bp + 2]
    mov [dbg_ip], ax
    inc word [dbg_count]
    pop ax
    pop bp
    iret
dbg_count: dw 0
dbg_ip: dw 0
dbg_flags_seen: dw 0
