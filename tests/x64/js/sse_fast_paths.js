#!/usr/bin/env node
// EXPECT: pass
// DESC: the fast paths of addps/subps/mulps (wasm SIMD, NaN and infinity rules, MXCSR flags) give what the exact
// per-element path gives, bit for bit, results and MXCSR, on random and special operands, interpreted and compiled;
// and the exact path what the host CPU gives (on x86 with a C compiler)

import fs from "node:fs";
import url from "node:url";
import { execFileSync } from "node:child_process";

const { V86 } = await import("../../../src/main.js");
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
process.on("unhandledRejection", exn => { throw exn; });

const N = 60000, IN = 0x100000, OUT = 0x800000;
// (" m": the memory form, " s": the same register twice); the bitwise and integer ones are compiled inline too
const OPS = ["addps", "subps", "mulps", "addpd", "subpd", "mulpd", "addss", "subss", "mulss", "addsd", "subsd", "mulsd",
    "addps m", "mulps m", "addss m", "mulss m", "subsd m", "mulsd m",
    "xorps", "andps", "andnps", "orps", "pxor", "pand", "pandn", "por", "xorpd", "andnpd",
    "paddb", "paddw", "paddd", "paddq", "psubb", "psubw", "psubd", "psubq",
    "pxor m", "pandn m", "paddd m", "xorps s", "pxor s", "psubd s", "andps s",
    // (" r": through eax, as cvttsd2si eax, xmm0 / cvtsi2sd xmm0, eax)
    "cvttsd2si r", "cvtsi2sd r"];
const tmp = fs.mkdtempSync("/tmp/v86-sse-");
fs.writeFileSync(tmp + "/t.asm", `
bits 16
org 0x7C00
    cli
    xor ax, ax
    mov ds, ax
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
    mov ss, ax
    mov esp, 0x90000
    mov eax, cr4
    or eax, 3 << 9                  ; OSFXSR, OSXMMEXCPT
    mov cr4, eax
    mov esi, ${IN}
    mov edi, ${OUT}
    mov ecx, ${N}
.l:
%assign k 0
%macro one 1
    ldmxcsr [esi + 32]
    movups xmm0, [esi]
    movups xmm1, [esi + 16]
    %1 xmm0, xmm1
    movups [edi + k * 20], xmm0
    stmxcsr [edi + k * 20 + 16]
%assign k k + 1
%endmacro
%macro onem 1
    ldmxcsr [esi + 32]
    movups xmm0, [esi]
    %1 xmm0, [esi + 16]
    movups [edi + k * 20], xmm0
    stmxcsr [edi + k * 20 + 16]
%assign k k + 1
%endmacro
%macro oner 1
    ldmxcsr [esi + 32]
    movups xmm0, [esi]
%ifidn %1, cvttsd2si
    cvttsd2si eax, xmm0
    movd xmm0, eax
%else
    mov eax, [esi + 16]
    cvtsi2sd xmm0, eax
%endif
    movups [edi + k * 20], xmm0
    stmxcsr [edi + k * 20 + 16]
%assign k k + 1
%endmacro
%macro ones 1
    ldmxcsr [esi + 32]
    movups xmm0, [esi]
    %1 xmm0, xmm0
    movups [edi + k * 20], xmm0
    stmxcsr [edi + k * 20 + 16]
%assign k k + 1
%endmacro
${OPS.map(o => "    " + ({ m: "onem", s: "ones", r: "oner" }[o.slice(-1)] && o[o.length - 2] === " " ? { m: "onem", s: "ones", r: "oner" }[o.slice(-1)] + " " + o.slice(0, -2) : "one " + o)).join("\n")}
    add esi, 48
    add edi, ${OPS.length * 20}
    dec ecx
    jnz .l
    mov dword [0x600], 0x45444F4E
.h: hlt
    jmp .h
align 8
gdt:
    dq 0, 0x00CF9A000000FFFF, 0x00CF92000000FFFF
gdt_ptr:
    dw 23
    dd gdt
`);
execFileSync("nasm", ["-f", "bin", tmp + "/t.asm", "-o", tmp + "/t.bin"]);
const bin = fs.readFileSync(tmp + "/t.bin");

// operands: random bits, and the special values (zeros, denormals, the smallest normals, huge values that overflow
// when added or multiplied, infinities, quiet and signaling NaNs), as singles and as doubles
let seed = 12345;
const rand = () => (seed = seed * 1103515245 + 12345 >>> 0) >>> 0;
const f32 = () => {
    const sign = rand() & 1 ? 0x80000000 : 0;
    switch(rand() % 12)
    {
        case 0: return sign;
        case 1: return sign | rand() % 0x800000;                       // denormal
        case 2: return sign | 0x00800000 | rand() % 0x800000;          // smallest normals
        case 3: return sign | 0x7F000000 | rand() % 0x800000;          // huge
        case 4: return sign | 0x7F800000;                              // infinity
        case 5: return sign | 0x7FC00000 | rand() % 0x400000;          // quiet NaN
        case 6: return sign | 0x7F800000 | 1 + rand() % 0x3FFFFF;      // signaling NaN
        case 7: return sign | 0x3F800000 | rand() % 0x800000;          // 1.0 ..
        default: return rand();
    }
};
const input = Buffer.alloc(N * 48);
for(let i = 0; i < N; i++)
{
    for(let k = 0; k < 8; k++) input.writeUInt32LE(f32() >>> 0, i * 48 + 4 * k);
    // (half of the cases: two singles combined as a double's halves already make every class of double)
    input.writeUInt32LE(0x1F80 | rand() % 64, i * 48 + 32);                 // the status flags: some already set
}

async function run(fast_off, jit)
{
    const emulator = new V86({ autostart: false, memory_size: 128 << 20, cpu_type: "x86", log_level: 0, disable_jit: !jit });
    await new Promise(r => emulator.add_listener("emulator-loaded", r));
    const cpu = emulator.v86.cpu;
    cpu.wm.exports["set_jit_config"](60, fast_off ? 1 : 0);
    cpu.reboot_internal();
    cpu.reset_memory();
    cpu.mem8.set(bin, 0x7C00);
    cpu.mem8.set(input, IN);
    cpu.mem8.set([0xEA, 0x00, 0x7C, 0x00, 0x00], 0xFFFF0);
    emulator.run();
    const t0 = Date.now();
    while(cpu.mem32s[0x600 >> 2] >>> 0 !== 0x45444F4E)
    {
        if(Date.now() - t0 > 120000) throw new Error("timeout");
        await new Promise(r => setTimeout(r, 50));
    }
    emulator.stop();
    const out = Buffer.from(cpu.mem8.slice(OUT, OUT + N * OPS.length * 20));
    emulator.destroy();
    return out;
}

const exact = await run(true, false);
// the exact path against the host CPU (x86 with a C compiler): the same instructions on the same operands
let failed = 0;
let native = "";
if(process.arch === "x64" && !process.env.NO_NATIVE)
{
    const ops = OPS.map((o, k) => o.endsWith(" m") ? `ONEM(${k}, "${o.slice(0, -2)}");` :
        o.endsWith(" s") ? `ONES(${k}, "${o.slice(0, -2)}");` :
        o === "cvttsd2si r" ? `ONER(${k}, "cvttsd2si %0, %%eax\\n\\tmovd %%eax, %0");` :
        o === "cvtsi2sd r" ? `ONER(${k}, "movl 16(%3), %%eax\\n\\tcvtsi2sd %%eax, %0");` : `ONE(${k}, "${o}");`).join(" ");
    fs.writeFileSync(tmp + "/native.c", `
#include <stdio.h>
#include <string.h>
#include <immintrin.h>
static unsigned char in[${N * 48}], out[${N * OPS.length * 20}];
#define ONE(k, INSN) do { unsigned m = *(unsigned *)(p + 32), res; __m128 a, b; memcpy(&a, p, 16); \\
    memcpy(&b, p + 16, 16); __asm__ volatile("ldmxcsr %2\\n\\t" INSN " %1, %0\\n\\tstmxcsr %3" \\
    : "+x"(a) : "x"(b), "m"(m), "m"(res)); memcpy(o + k * 20, &a, 16); memcpy(o + k * 20 + 16, &res, 4); } while(0)
#define ONER(k, INSN) do { unsigned m = *(unsigned *)(p + 32), res; __m128 a; memcpy(&a, p, 16); \\
    __asm__ volatile("ldmxcsr %1\\n\\t" INSN "\\n\\tstmxcsr %2" \\
    : "+x"(a) : "m"(m), "m"(res), "r"(p) : "eax"); memcpy(o + k * 20, &a, 16); memcpy(o + k * 20 + 16, &res, 4); } while(0)
#define ONES(k, INSN) do { unsigned m = *(unsigned *)(p + 32), res; __m128 a; memcpy(&a, p, 16); \\
    __asm__ volatile("ldmxcsr %1\\n\\t" INSN " %0, %0\\n\\tstmxcsr %2" \\
    : "+x"(a) : "m"(m), "m"(res)); memcpy(o + k * 20, &a, 16); memcpy(o + k * 20 + 16, &res, 4); } while(0)
#define ONEM(k, INSN) do { unsigned m = *(unsigned *)(p + 32), res; __m128 a, b; memcpy(&a, p, 16); \\
    memcpy(&b, p + 16, 16); __asm__ volatile("ldmxcsr %2\\n\\t" INSN " %1, %0\\n\\tstmxcsr %3" \\
    : "+x"(a) : "m"(b), "m"(m), "m"(res)); memcpy(o + k * 20, &a, 16); memcpy(o + k * 20 + 16, &res, 4); } while(0)
int main(void) {
    if(fread(in, 1, sizeof in, stdin) != sizeof in) return 1;
    for(int i = 0; i < ${N}; i++) { unsigned char *p = in + i * 48, *o = out + i * ${OPS.length * 20}; ${ops} }
    fwrite(out, 1, sizeof out, stdout);
    return 0;
}
`);
    try
    {
        execFileSync("cc", ["-O1", "-o", tmp + "/native", tmp + "/native.c"], { stdio: "ignore" });
        const cpu = execFileSync(tmp + "/native", { input, maxBuffer: 1 << 26 });
        for(let i = 0; i < N * OPS.length && failed < 10; i++)
        {
            if(!cpu.subarray(i * 20, i * 20 + 20).equals(exact.subarray(i * 20, i * 20 + 20)))
            {
                const c = Math.floor(i / OPS.length);
                console.log(`host CPU ${OPS[i % OPS.length]} case ${c}: operands ${input.subarray(c * 48, c * 48 + 36).toString("hex")}\n` +
                    `  cpu ${cpu.subarray(i * 20, i * 20 + 20).toString("hex")}\n  v86 ${exact.subarray(i * 20, i * 20 + 20).toString("hex")}`);
                failed++;
            }
        }
        native = ", as the host CPU";
    }
    catch(e)
    {
        native = " (no C compiler: not compared with the host CPU)";
    }
}
for(const [name, out] of [["interpreted", await run(false, false)], ["compiled", await run(false, true)]])
{
    for(let i = 0; i < N * OPS.length && failed < 10; i++)
    {
        const a = exact.subarray(i * 20, i * 20 + 20), b = out.subarray(i * 20, i * 20 + 20);
        if(!a.equals(b))
        {
            const c = Math.floor(i / OPS.length);
            console.log(`${name} ${OPS[i % OPS.length]} case ${c}: operands ${input.subarray(c * 48, c * 48 + 36).toString("hex")}\n` +
                `  exact ${a.toString("hex")}\n  fast  ${b.toString("hex")}`);
            failed++;
        }
    }
}
fs.rmSync(tmp, { recursive: true });
console.log(failed ? "FAIL" : `${N} cases of ${OPS.length} operations: the same${native}`);
process.exit(failed ? 1 : 0);
