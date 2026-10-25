; EXPECT: flaky
; ISSUE: copy/v86#1614
; DESC: PIT channel 0 at 100 Hz delivers ~100 IRQ0 per RTC second (guest clock must not run slow)
; SERIAL: yes
; Takes about 2 seconds of wall clock time and is sensitive to cpu load (it fails when the host
; cannot run the emulator at full speed, which is how the issue shows up), so it runs alone.
%include "inc/pm32.inc"

%define TICKS 0x700

%macro RTC_SECONDS 0         ; -> al
    mov al, 0
    out 0x70, al
    in al, 0x71
%endmacro

test_main:
    mov dword [0xFEE00350], 0x700               ; LINT0 = ExtINT so PIC interrupts reach the cpu
    mov al, 0x11
    out 0x20, al
    mov al, 0x20
    out 0x21, al
    mov al, 0x04
    out 0x21, al
    mov al, 0x01
    out 0x21, al
    mov al, 0xFE
    out 0x21, al
    mov al, 0xFF
    out 0xA1, al
    mov edi, IDT_BASE + 0x20 * 8
    mov eax, irq0
    mov word [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov word [edi + 6], ax
    mov word [idt_ptr], 0x21 * 8 - 1
    lidt [idt_ptr]
    ; PIT channel 0, mode 2, divisor 11932 => 1193182 / 11932 = 100.0 Hz
    mov al, 0x34
    out 0x43, al
    mov al, 11932 & 0xFF
    out 0x40, al
    mov al, 11932 >> 8
    out 0x40, al
    ; synchronise on a seconds rollover of the RTC
    RTC_SECONDS
    mov bl, al
.wait1:
    RTC_SECONDS
    cmp al, bl
    je .wait1
    mov bl, al
    mov dword [TICKS], 0
    sti
.wait2:
    RTC_SECONDS
    cmp al, bl
    je .wait2
    cli
    mov eax, [TICKS]
    cmp eax, 90
    jae .lo_ok
    mov [RESULT_ACT], eax
    FAIL16 1                                    ; clock too slow
.lo_ok:
    cmp eax, 110
    jbe .hi_ok
    mov [RESULT_ACT], eax
    FAIL16 2                                    ; clock too fast
.hi_ok:
    PASS

irq0:
    push eax
    inc dword [TICKS]
    mov al, 0x20
    out 0x20, al
    pop eax
    iretd
