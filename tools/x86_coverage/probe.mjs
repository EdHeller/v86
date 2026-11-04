#!/usr/bin/env node
// Builds the probe of tools/x86_coverage: every instruction form of reference-<bits>.json (python3
// tools/x86_coverage/reference.py) executed once, recording the exception it raised (-1 for none, 6 for #UD, ...).
// A boot stub at 0x7C00 enters the mode, masks the PICs and installs handlers that record the vector, reload the
// segments and the stack and continue with the next probe; the probes are at 1 MiB, the results (one dword per probe)
// at 8 MiB, scratch memory at 9 MiB, and after each probe that raised nothing its state at 16 MiB + 256 * index:
// eax..edi (32 bytes), eflags (at 32), xmm0..7 (at 48) and the first 64 bytes of scratch memory (at 176). Done: the magic 0x45444F4E ("NODE") at 0x600, the probe count at 0x604; the
// probe running now is at 0x608 (to find one that hangs). The whole set runs [0x634] times (at least once: the JIT
// compiles the probes' pages after a few passes), counted at 0x630.
//
//   node tools/x86_coverage/probe.mjs [bits]     writes build/x86_coverage/probe<bits>-stub.bin, -probes.bin, -forms.json
//
// Control flow and instructions that change what the probe stands on (descriptor tables, control registers, MSRs,
// waits) are not probed; their forms are listed with outcome "skip" (tests/x64 and tests/nasm cover them by hand).

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { execFileSync } from "node:child_process";

const ROOT = path.join(path.dirname(url.fileURLToPath(import.meta.url)), "../..");
const OUT = path.join(ROOT, "build/x86_coverage");
const bits = +process.argv[2] || 32;
if(![16, 32, 64].includes(bits)) throw new Error("bits: 16, 32 or 64");

// (64-bit: four times the forms, so a shared dump routine instead of one per probe, and 256 MiB of memory; a dump is
// rax..r15 (128 bytes), rflags at 128, xmm0..15 at 144 and the 64 bytes of scratch memory at 400)
const LAYOUT = {
    16: { PROBES: 0x100000, RESULTS: 0x800000, SCRATCH: 0x900000, DUMPS: 0x1000000, DUMP_SIZE: 256, MEMORY_MB: 64 },
    32: { PROBES: 0x100000, RESULTS: 0x800000, SCRATCH: 0x900000, DUMPS: 0x1000000, DUMP_SIZE: 256, MEMORY_MB: 64 },
    64: { PROBES: 0x1000000, RESULTS: 0x800000, SCRATCH: 0x900000, DUMPS: 0x4000000, DUMP_SIZE: 512, MEMORY_MB: 256 },
}[bits];
const { PROBES, RESULTS, SCRATCH, DUMPS, DUMP_SIZE } = LAYOUT;

// control flow, waits, and what changes the probe's own environment
const SKIP_MNEMONICS = new Set(`
jmp call ret retf retn iret iretd iretw iretq int int1 int3 into hlt loop loope loopne loopz loopnz jcxz jecxz jrcxz
ja jae jb jbe jc je jg jge jl jle jna jnae jnb jnbe jnc jne jng jnge jnl jnle jno jnp jns jnz jo jp jpe jpo js jz
sysenter sysexit syscall sysret sysretd sysexitd lgdt lidt lldt ltr lmsw wrmsr mwait mwaitx umwait tpause xbegin xabort
rsm enclu encls enclv vmlaunch vmresume vmcall vmfunc vmxoff vmxon vmptrld vmclear vmrun vmmcall vmload vmsave stgi
clgi skinit invlpga invlpgb tlbsync uiret senduipi clui stui hreset seamcall seamops seamret tdcall pconfig
`.trim().split(/\s+/));

function skip(form)
{
    if(SKIP_MNEMONICS.has(form.mnemonic)) return true;
    // (objdump's size-suffixed spellings: lgdtd, retw, iretd, callw, ...)
    if(/^(j|loop|ret|iret|call|lgdt|lidt|lldt|ltr|lmsw|int|sysret|sysexit|sysenter|syscall)/.test(form.mnemonic) &&
        !/^(insb|insw|insd|invd|invlpg|invpcid|invept|invvpid)/.test(form.mnemonic)) return true;
    const b = form.full;
    // mov to and from control and debug registers, the test registers
    if(/^(66|f2|f3)?(4[0-9a-f])?0f2[0-7]/.test(b)) return true;
    return false;
}

function forms()
{
    const ref = JSON.parse(fs.readFileSync(path.join(ROOT, `tools/x86_coverage/reference-${bits}.json`), "utf8"));
    const seen = new Map(), fulls = new Set();     // (one probe per distinct instruction)
    for(const r of ref)
    {
        const b = Buffer.from(r.bytes, "hex");
        const n = { "1": 1, "0f": 2, "0f38": 3, "0f3a": 3 }[r.map];
        let p = [0x66, 0xF2, 0xF3].includes(b[0]) ? 1 : 0;
        if(bits === 64 && (b[p] & 0xF0) === 0x40) p++;     // REX
        const op = b.subarray(0, p + n).toString("hex");
        const reg = b[p + n] >> 3 & 7;
        const key = op + "/" + reg + "/" + r.form + "/" + r.mnemonic;
        // (objdump's "(bad)" operand: its length is unknown, so int3s follow the opcode bytes: an implementation that
        // decodes it anyway stops at one with #BP instead of running on into the next probe)
        const full = r.text.includes("(bad)") ? r.bytes + "cc".repeat(8) : r.full;
        if(!seen.has(key) && !fulls.has(full)) seen.set(key, { key, op, reg, form: r.form, mnemonic: r.mnemonic, text: r.text, map: r.map, full });
        fulls.add(full);
    }
    return [...seen.values()];
}

const all = forms();
const probed = all.filter(f => !skip(f));
fs.mkdirSync(OUT, { recursive: true });

let stub, probes;
if(bits === 32)
{
    stub = `
bits 16
org 0x7C00
    cli
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7000
    ; mask both PICs: a probed sti lets nothing in
    mov al, 0xFF
    out 0x21, al
    out 0xA1, al
    lgdt [gdt_ptr]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:pm
bits 32
pm:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000
    ; the IDT: 32 vectors, each recording its number
    mov edi, 0x20000
    mov ecx, 0
.idt:
    mov eax, ecx
    imul eax, eax, 16
    add eax, vec0
    mov word [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov word [edi + 6], ax
    add edi, 8
    inc ecx
    cmp ecx, 32
    jb .idt
    lidt [idt_ptr]
    ; SSE on (CR0.EM off, MP on; CR4.OSFXSR, OSXMMEXCPT, OSXSAVE), x87 initialized
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, 1 << 1
    mov cr0, eax
    mov eax, cr4
    or eax, (1 << 9) | (1 << 10) | (1 << 18)    ; OSFXSR, OSXMMEXCPT, OSXSAVE
    mov cr4, eax
    fninit
    mov dword [0x608], -1
    jmp ${PROBES}

    ; the state each probe starts from
reset:
    pop dword [0x60C]
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000
    push dword 2
    popfd
    ldmxcsr [mxcsr]
    ; mm0..7 from the pattern (the x87 registers' mantissas), then an empty x87 stack over them
%assign k 0
%rep 8
    movq mm%+k, [pattern + 0x80 + k * 8]
%assign k k + 1
%endrep
    fninit
    mov esi, pattern
    mov edi, ${SCRATCH}
    mov ecx, 256
    rep movsd
    movdqu xmm0, [pattern + 0x00]
    movdqu xmm1, [pattern + 0x10]
    movdqu xmm2, [pattern + 0x20]
    movdqu xmm3, [pattern + 0x30]
    movdqu xmm4, [pattern + 0x40]
    movdqu xmm5, [pattern + 0x50]
    movdqu xmm6, [pattern + 0x60]
    movdqu xmm7, [pattern + 0x70]
    mov eax, ${SCRATCH}
    mov ebx, ${SCRATCH + 64}
    mov ecx, 4
    mov edx, 0x1E00                ; an unused port (in, out, ins, outs)
    mov esi, ${SCRATCH + 128}
    mov edi, ${SCRATCH + 256}
    mov ebp, ${SCRATCH + 512}
    jmp [0x60C]

align 16
vec0:
%assign v 0
%rep 32
    mov dword [ss:0x610], v
    jmp handler
    align 16
%assign v v + 1
%endrep
handler:
    mov ax, 0x10
    mov ds, ax
    mov ss, ax
    mov esp, 0x90000
    mov eax, [0x610]
    mov [0x614], eax
    push dword 2
    popfd
    jmp [0x618]

align 4
mxcsr: dd 0x1F80
align 16
pattern:
%assign k 0
%rep 64
    dd 0x3F800000 + k * 0x10000 + k     ; floats from 1.0 upward
    dd (k * 0x9E3779B9) & 0xFFFFFFFF
    dd 0x40000000 + k * 0x1000          ; and a double's high half: 2.0 upward
    dd (k * 0x85EBCA6B + 0x1234) & 0xFFFFFFFF
%assign k k + 1
%endrep
align 8
gdt:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt
idt_ptr:
    dw 32 * 8 - 1
    dd 0x20000
`;

    // each probe: where to continue, the reset, the instruction, its outcome
    probes = `bits 32\norg ${PROBES}\n`;
    probed.forEach((f, i) => {
        probes += `p${i}:\n    mov dword [0x608], ${i}\n    mov dword [0x618], e${i}\n    mov dword [0x614], -1\n    call ${0x7C00}+reset_off\n`;
        probes += `    db ${Buffer.from(f.full, "hex").map(x => x).join(",").split(",").map(x => "0x" + (+x).toString(16)).join(",")}\n`;
        const d = DUMPS + i * DUMP_SIZE;
        probes += `e${i}:\n    mov [ss:${d}], eax\n    mov [ss:${d + 4}], ecx\n    mov [ss:${d + 8}], edx\n    mov [ss:${d + 12}], ebx\n` +
            `    mov [ss:${d + 16}], esp\n    mov [ss:${d + 20}], ebp\n    mov [ss:${d + 24}], esi\n    mov [ss:${d + 28}], edi\n` +
            `    mov ax, 0x10\n    mov ds, ax\n    mov es, ax\n    mov ss, ax\n    mov esp, 0x90000\n    pushfd\n    pop dword [${d + 32}]\n` +
            [0, 1, 2, 3, 4, 5, 6, 7].map(k => `    movdqu [${d + 48 + 16 * k}], xmm${k}\n`).join("") +
            `    mov esi, ${SCRATCH}\n    mov edi, ${d + 176}\n    mov ecx, 16\n    rep movsd\n` +
            `    mov eax, [0x614]\n    mov [${RESULTS + i * 4}], eax\n`;
    });
    probes += `    mov eax, [0x630]\n    inc eax\n    mov [0x630], eax\n    cmp eax, [0x634]\n    jb p0\n` +
        `    mov dword [0x604], ${probed.length}\n    mov dword [0x600], 0x45444F4E\n.done:\n    cli\n    hlt\n    jmp .done\n`;
}
else if(bits === 16)
{
    // 16-bit code in protected mode: the probes in 16-bit code segments (one per 64 KiB chunk, selectors 0x20 + 8k),
    // calling the 32-bit reset and dump routines of the stub far. ss is the flat data segment (esp 0x90000), ds and
    // es the scratch segment (base SCRATCH), so the memory forms' [bx+si] (bx = si = 0) is the start of scratch.
    // Control words: 0x608 the probe's index, 0x614 its outcome, 0x618 m16:32 of the next probe, 0x620 the return of
    // the reset.
    const PER_CHUNK = 1200, chunks = Math.ceil(probed.length / PER_CHUNK);
    stub = `
bits 16
org 0x7C00
    cli
    xor ax, ax
    mov ds, ax
    mov ss, ax
    mov sp, 0x7000
    mov al, 0xFF
    out 0x21, al
    out 0xA1, al
    lgdt [gdt_ptr]
    mov eax, cr0
    or eax, 1
    mov cr0, eax
    jmp 0x08:pm
bits 32
pm:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000
    mov edi, 0x20000
    mov ecx, 0
.idt:
    mov eax, ecx
    imul eax, eax, 16
    add eax, vec0
    mov word [edi], ax
    mov word [edi + 2], 0x08
    mov word [edi + 4], 0x8E00
    shr eax, 16
    mov word [edi + 6], ax
    add edi, 8
    inc ecx
    cmp ecx, 32
    jb .idt
    lidt [idt_ptr]
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, 1 << 1
    mov cr0, eax
    mov eax, cr4
    or eax, (1 << 9) | (1 << 10) | (1 << 18)
    mov cr4, eax
    fninit
    mov dword [0x608], -1
    jmp 0x20:0

; the state each probe starts from (called far from the probe: eip and cs on the stack)
reset:
    pop dword [ss:0x620]
    pop dword [ss:0x624]
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov esp, 0x90000
    mov dword [0x614], -1
    push dword 2
    popfd
    ldmxcsr [mxcsr]
%assign k 0
%rep 8
    movq mm%+k, [pattern + 0x80 + k * 8]
%assign k k + 1
%endrep
    fninit
    cld
    mov esi, pattern
    mov edi, ${SCRATCH}
    mov ecx, 256
    rep movsd
%assign k 0
%rep 8
    movdqu xmm%+k, [pattern + k * 16]
%assign k k + 1
%endrep
    mov ax, 0x18
    mov ds, ax
    mov es, ax
    mov eax, 0x3F800040
    mov ecx, 4
    mov edx, 0x1E00                ; an unused port
    xor ebx, ebx
    xor esi, esi                   ; ([bx+si]: scratch)
    mov edi, 0x100
    mov ebp, 0x200
    jmp far [ss:0x620]

; after a probe that raised nothing: its state to DUMPS + DUMP_SIZE * index (the flags before anything changes them)
dump:
    mov [ss:0x700], eax
    mov eax, [ss:0x608]
    lea eax, [eax * 8]
    lea eax, [eax * 8]
    lea eax, [eax * 4 + ${DUMPS}]
    mov [ss:eax + 4], ecx
    mov [ss:eax + 8], edx
    mov [ss:eax + 12], ebx
    mov [ss:eax + 16], esp
    mov [ss:eax + 20], ebp
    mov [ss:eax + 24], esi
    mov [ss:eax + 28], edi
    mov ecx, [ss:0x700]
    mov [ss:eax], ecx
    mov cx, 0x10
    mov ss, cx
    mov esp, 0x90000
    pushfd
    pop dword [ss:eax + 32]
    mov ds, cx
    mov es, cx
    cld
%assign k 0
%rep 8
    movdqu [eax + 48 + k * 16], xmm%+k
%assign k k + 1
%endrep
    mov esi, ${SCRATCH}
    lea edi, [eax + 176]
    mov ecx, 16
    rep movsd
    jmp after

align 16
vec0:
%assign v 0
%rep 32
    mov dword [ss:0x614], v
    jmp after
    align 16
%assign v v + 1
%endrep
; the outcome (the vector, or -1) to RESULTS + 4 * index, then the next probe
after:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov esp, 0x90000
    mov eax, [0x608]
    mov edx, [0x614]
    mov [${RESULTS} + eax * 4], edx
    push dword 2
    popfd
    jmp far [0x618]

align 4
mxcsr: dd 0x1F80
align 16
pattern:
%assign k 0
%rep 64
    dd 0x3F800000 + k * 0x10000 + k
    dd (k * 0x9E3779B9) & 0xFFFFFFFF
    dd 0x40000000 + k * 0x1000
    dd (k * 0x85EBCA6B + 0x1234) & 0xFFFFFFFF
%assign k k + 1
%endrep
align 8
gdt:
    dq 0
    dq 0x00CF9A000000FFFF
    dq 0x00CF92000000FFFF
    dw 0xFFFF, ${SCRATCH} & 0xFFFF            ; 0x18: the scratch segment
    db (${SCRATCH} >> 16) & 0xFF, 0x92, 0xCF, ${SCRATCH} >>> 24
%assign k 0
%rep ${chunks}
    dw 0xFFFF, (${PROBES} + k * 0x10000) & 0xFFFF   ; 0x20 + 8k: a 16-bit code segment per chunk
    db ((${PROBES} + k * 0x10000) >> 16) & 0xFF, 0x9A, 0x00, ((${PROBES} + k * 0x10000) >> 24) & 0xFF
%assign k k + 1
%endrep
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt
idt_ptr:
    dw 32 * 8 - 1
    dd 0x20000
`;

    probes = "bits 16\n";
    probed.forEach((f, i) => {
        const k = Math.floor(i / PER_CHUNK);
        if(i % PER_CHUNK === 0) probes += `section c${k} start=${k * 0x10000} vstart=0\n`;
        const last = (i + 1) % PER_CHUNK === 0 || i + 1 === probed.length;
        const next_sel = 0x20 + 8 * (i + 1 === probed.length ? k : Math.floor((i + 1) / PER_CHUNK));
        probes += `p${i}:\n    mov dword [ss:0x608], ${i}\n    mov dword [ss:0x618], ${last && i + 1 < probed.length ? 0 : "p" + (i + 1)}\n` +
            `    mov word [ss:0x61C], ${next_sel}\n    call dword 0x08:${0x7C00}+reset_off\n` +
            `    db ${[...Buffer.from(f.full, "hex")].map(x => "0x" + x.toString(16)).join(",")}\n    jmp dword 0x08:${0x7C00}+dump_off\n`;
    });
    probes += `p${probed.length}:\n    mov eax, [ss:0x630]\n    inc eax\n    mov [ss:0x630], eax\n    cmp eax, [ss:0x634]\n` +
        `    jae .fin\n    jmp 0x20:0\n.fin:\n    mov dword [ss:0x604], ${probed.length}\n    mov dword [ss:0x600], 0x45444F4E\n.done:\n    cli\n    hlt\n    jmp .done\n`;
}
else
{
    // long mode: 1 GiB identity-mapped with 2 MiB pages, a 64-bit TSS whose IST1 stack every exception gate uses (a
    // probe that leaves rsp unusable cannot turn an exception into a triple fault)
    stub = `
bits 16
org 0x7C00
    cli
    xor ax, ax
    mov ds, ax
    mov es, ax
    mov ss, ax
    mov sp, 0x7000
    cld
    mov al, 0xFF
    out 0x21, al
    out 0xA1, al
    ; page tables at 0x2000 (PML4), 0x3000 (PDPT), 0x4000 (PD), the TSS at 0x5000
    mov di, 0x2000
    xor ax, ax
    mov cx, 4 * 4096 / 2
    rep stosw
    mov dword [0x2000], 0x3000 | 3
    mov dword [0x3000], 0x4000 | 3
    mov di, 0x4000
    mov eax, 0x83
    xor edx, edx
    mov cx, 512
.pd:
    mov [di], eax
    mov [di + 4], edx
    add di, 8
    add eax, 0x200000
    adc edx, 0
    loop .pd
    mov dword [0x5000 + 36], 0x70000            ; IST1
    lgdt [gdt_ptr]
    mov eax, cr4
    or eax, (1 << 5) | (1 << 9) | (1 << 10) | (1 << 18)     ; PAE, OSFXSR, OSXMMEXCPT, OSXSAVE
    mov cr4, eax
    mov eax, 0x2000
    mov cr3, eax
    mov ecx, 0xC0000080
    rdmsr
    or eax, 1 << 8                              ; LME
    wrmsr
    mov eax, cr0
    and eax, ~(1 << 2)
    or eax, (1 << 31) | (1 << 1) | 1            ; PG, MP, PE
    mov cr0, eax
    jmp dword 0x08:lm
bits 64
lm:
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov rsp, 0x90000
    mov ax, 0x18
    ltr ax
    ; the IDT: 32 vectors, each recording its number, all on IST1
    mov edi, 0x20000
    xor ecx, ecx
.idt:
    mov eax, ecx
    imul eax, eax, 16
    add eax, vec0
    mov word [rdi], ax
    mov word [rdi + 2], 0x08
    mov word [rdi + 4], 0x8E01
    shr eax, 16
    mov word [rdi + 6], ax
    mov dword [rdi + 8], 0
    mov dword [rdi + 12], 0
    add edi, 16
    inc ecx
    cmp ecx, 32
    jb .idt
    lidt [idt_ptr]
    fninit
    mov dword [0x608], -1
    mov rax, ${PROBES}
    jmp rax

; the state each probe starts from
reset:
    pop qword [0x60C]
    mov ax, 0x10
    mov ds, ax
    mov es, ax
    mov fs, ax
    mov gs, ax
    mov ss, ax
    mov rsp, 0x90000
    mov dword [0x614], -1
    push qword 2
    popfq
    ldmxcsr [mxcsr]
%assign k 0
%rep 8
    movq mm%+k, [pattern + 0x80 + k * 8]
%assign k k + 1
%endrep
    fninit
    mov esi, pattern
    mov edi, ${SCRATCH}
    mov ecx, 256
    rep movsd
%assign k 0
%rep 16
    movdqu xmm%+k, [pattern + k * 16]
%assign k k + 1
%endrep
    mov eax, ${SCRATCH}
    mov ebx, ${SCRATCH + 64}
    mov ecx, 4
    mov edx, 0x1E00                ; an unused port (in, out, ins, outs)
    mov esi, ${SCRATCH + 128}
    mov edi, ${SCRATCH + 256}
    mov ebp, ${SCRATCH + 512}
    mov r8d, ${SCRATCH}            ; (the memory forms with REX.B: [r8])
    mov r9d, ${SCRATCH + 64}
    mov r10d, 4
    mov r11, [pattern + 0x104]
    mov r12, [pattern + 0x11C]
    mov r13, [pattern + 0x134]
    mov r14, [pattern + 0x14C]
    mov r15, [pattern + 0x164]
    jmp [0x60C]

; after a probe that raised nothing: its state to DUMPS + DUMP_SIZE * index (the flags before anything changes them)
dump:
    mov [0x700], rax
    mov eax, [0x608]
    lea rax, [rax * 8]
    lea rax, [rax * 8]
    lea rax, [rax * 8 + ${DUMPS}]
    mov [rax + 8], rcx
    mov [rax + 16], rdx
    mov [rax + 24], rbx
    mov [rax + 32], rsp
    mov [rax + 40], rbp
    mov [rax + 48], rsi
    mov [rax + 56], rdi
%assign k 8
%rep 8
    mov [rax + k * 8], r%+k
%assign k k + 1
%endrep
    mov rcx, [0x700]
    mov [rax], rcx
    mov rsp, 0x90000
    pushfq
    pop qword [rax + 128]
    mov cx, 0x10
    mov ds, cx
    mov es, cx
    mov ss, cx
    cld
%assign k 0
%rep 16
    movdqu [rax + 144 + k * 16], xmm%+k
%assign k k + 1
%endrep
    mov esi, ${SCRATCH}
    lea rdi, [rax + 400]
    mov ecx, 16
    rep movsd
    jmp after

align 16
vec0:
%assign v 0
%rep 32
    mov dword [0x614], v
    jmp after
    align 16
%assign v v + 1
%endrep
; the outcome (the vector, or -1) to RESULTS + 4 * index, then the next probe
after:
    mov ax, 0x10
    mov ds, ax
    mov ss, ax
    mov rsp, 0x90000
    mov eax, [0x608]
    mov edx, [0x614]
    mov [${RESULTS} + rax * 4], edx
    push qword 2
    popfq
    jmp [0x618]

align 4
mxcsr: dd 0x1F80
align 16
pattern:
%assign k 0
%rep 64
    dd 0x3F800000 + k * 0x10000 + k     ; floats from 1.0 upward
    dd (k * 0x9E3779B9) & 0xFFFFFFFF
    dd 0x40000000 + k * 0x1000          ; and a double's high half: 2.0 upward
    dd (k * 0x85EBCA6B + 0x1234) & 0xFFFFFFFF
%assign k k + 1
%endrep
align 8
gdt:
    dq 0
    dq 0x00AF9A000000FFFF
    dq 0x00CF92000000FFFF
    dq 0x0000890050000067               ; the TSS at 0x5000
    dq 0
gdt_end:
gdt_ptr:
    dw gdt_end - gdt - 1
    dd gdt
idt_ptr:
    dw 32 * 16 - 1
    dq 0x20000
`;

    // each probe: its index, where to go next, the reset, the instruction, the dump
    probes = `bits 64\norg ${PROBES}\n`;
    probed.forEach((f, i) => {
        probes += `p${i}:\n    mov dword [0x608], ${i}\n    mov qword [0x618], p${i + 1}\n    call ${0x7C00}+reset_off\n` +
            `    db ${[...Buffer.from(f.full, "hex")].map(x => "0x" + x.toString(16)).join(",")}\n    jmp ${0x7C00}+dump_off\n`;
    });
    probes += `p${probed.length}:\n    mov eax, [0x630]\n    inc eax\n    mov [0x630], eax\n    cmp eax, [0x634]\n    jb p0\n` +
        `    mov dword [0x604], ${probed.length}\n    mov dword [0x600], 0x45444F4E\n.done:\n    cli\n    hlt\n    jmp .done\n`;
}

const stub_asm = path.join(OUT, `probe${bits}-stub.asm`);
fs.writeFileSync(stub_asm, stub);
execFileSync("nasm", ["-f", "bin", "-o", path.join(OUT, `probe${bits}-stub.bin`), "-l", path.join(OUT, `probe${bits}-stub.lst`), stub_asm]);
// (the offsets of `reset` and `dump` in the stub, from its listing)
const lst = fs.readFileSync(path.join(OUT, `probe${bits}-stub.lst`), "utf8").split("\n");
const offset_of = label => {
    const at = lst.findIndex(l => new RegExp("^\\s*\\d+\\s+" + label + ":").test(l));
    return at < 0 ? 0 : parseInt(lst.slice(at + 1).find(l => /^\s*\d+\s+[0-9A-F]{8}\s/.test(l)).trim().split(/\s+/)[1], 16);
};
const probes_asm = path.join(OUT, `probe${bits}-probes.asm`);
fs.writeFileSync(probes_asm, probes.replace(/reset_off/g, "0x" + offset_of("reset").toString(16)).replace(/dump_off/g, "0x" + offset_of("dump").toString(16)));
execFileSync("nasm", ["-f", "bin", "-O0", "-o", path.join(OUT, `probe${bits}-probes.bin`), probes_asm], { maxBuffer: 1 << 28 });
fs.writeFileSync(path.join(OUT, `probe${bits}-forms.json`), JSON.stringify({ layout: LAYOUT, probed, skipped: all.filter(skip) }));
console.log(`${bits}-bit: ${all.length} forms, ${probed.length} probed, ${all.length - probed.length} skipped; ` +
    `probes ${fs.statSync(path.join(OUT, `probe${bits}-probes.bin`)).size} bytes`);
