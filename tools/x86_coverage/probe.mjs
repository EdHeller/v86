#!/usr/bin/env node
// Builds the probe of tools/x86_coverage: every instruction form of reference-<bits>.json (python3
// tools/x86_coverage/reference.py) executed once, recording the exception it raised (-1 for none, 6 for #UD, ...).
// A boot stub at 0x7C00 enters the mode, masks the PICs and installs handlers that record the vector, reload the
// segments and the stack and continue with the next probe; the probes are at 1 MiB, the results (one dword per probe)
// at 8 MiB, scratch memory at 9 MiB, and after each probe that raised nothing its state at 16 MiB + 256 * index:
// eax..edi (32 bytes), eflags (at 32), xmm0..7 (at 48) and the first 64 bytes of scratch memory (at 176). Done: the magic 0x45444F4E ("NODE") at 0x600, the probe count at 0x604; the
// probe running now is at 0x608 (to find one that hangs).
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
if(bits !== 32) throw new Error("only the 32-bit probe so far");

export const PROBES = 0x100000, RESULTS = 0x800000, SCRATCH = 0x900000, DUMPS = 0x1000000, DUMP_SIZE = 256;

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
    if(/^(66|f2|f3)?0f2[0-7]/.test(b)) return true;
    return false;
}

function forms()
{
    const ref = JSON.parse(fs.readFileSync(path.join(ROOT, `tools/x86_coverage/reference-${bits}.json`), "utf8"));
    const seen = new Map();
    for(const r of ref)
    {
        const b = Buffer.from(r.bytes, "hex");
        const n = { "1": 1, "0f": 2, "0f38": 3, "0f3a": 3 }[r.map];
        const p = [0x66, 0xF2, 0xF3].includes(b[0]) ? 1 : 0;
        const op = b.subarray(0, p + n).toString("hex");
        const reg = b[p + n] >> 3 & 7;
        const key = op + "/" + reg + "/" + r.form + "/" + r.mnemonic;
        if(!seen.has(key)) seen.set(key, { key, op, reg, form: r.form, mnemonic: r.mnemonic, text: r.text, map: r.map, full: r.full });
    }
    return [...seen.values()];
}

const all = forms();
const probed = all.filter(f => !skip(f));
fs.mkdirSync(OUT, { recursive: true });

const stub = `
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
let probes = `bits 32\norg ${PROBES}\n`;
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
probes += `    mov dword [0x604], ${probed.length}\n    mov dword [0x600], 0x45444F4E\n.done:\n    cli\n    hlt\n    jmp .done\n`;

const stub_asm = path.join(OUT, `probe${bits}-stub.asm`);
fs.writeFileSync(stub_asm, stub);
execFileSync("nasm", ["-f", "bin", "-o", path.join(OUT, `probe${bits}-stub.bin`), "-l", path.join(OUT, `probe${bits}-stub.lst`), stub_asm]);
// (the offset of `reset` in the stub, from its listing)
const lst = fs.readFileSync(path.join(OUT, `probe${bits}-stub.lst`), "utf8");
const reset_line = lst.split("\n").findIndex(l => /^\s*\d+\s+reset:/.test(l));
const reset_off = parseInt(lst.split("\n").slice(reset_line + 1).find(l => /^\s*\d+\s+[0-9A-F]{8}\s/.test(l)).trim().split(/\s+/)[1], 16);
const probes_asm = path.join(OUT, `probe${bits}-probes.asm`);
fs.writeFileSync(probes_asm, probes.replace(/reset_off/g, "0x" + reset_off.toString(16)));
execFileSync("nasm", ["-f", "bin", "-O0", "-o", path.join(OUT, `probe${bits}-probes.bin`), probes_asm], { maxBuffer: 1 << 28 });
fs.writeFileSync(path.join(OUT, `probe${bits}-forms.json`), JSON.stringify({ probed, skipped: all.filter(skip) }));
console.log(`${bits}-bit: ${all.length} forms, ${probed.length} probed, ${all.length - probed.length} skipped; ` +
    `probes ${fs.statSync(path.join(OUT, `probe${bits}-probes.bin`)).size} bytes`);
