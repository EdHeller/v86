import fs from "node:fs";
const { V86 } = await import("/home/user/v86/src/main.js");
const buf = p => { const b = fs.readFileSync(p); return b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength); };
const emulator = new V86({
    bios: { buffer: buf("/home/user/v86/bios/seabios.bin") }, vga_bios: { buffer: buf("/home/user/v86/bios/vgabios.bin") },
    cdrom: { buffer: buf("/root/jitwork/TempleOS-suite.ISO") }, initial_state: { buffer: buf("/root/jitwork/pre-dungen.state") },
    memory_size: 512 * 1024 * 1024, vga_memory_size: 8 * 1024 * 1024, autostart: true,
    jit64_threshold: process.env.JIT64 === undefined ? undefined : +process.env.JIT64,
    jit64_disable_mask: +process.env.JIT64_MASK || 0,
    wasm_path: "/home/user/v86/build/v86.wasm", log_level: 0 });
const wm = () => emulator.v86.cpu.wm.exports;
emulator.add_listener("emulator-ready", () => {
    if(process.env.VERIFY) wm().set_jit_config(46, 1);
    if(process.env.LO) wm().set_jit_config(44, parseInt(process.env.LO, 16));
    if(process.env.HI) wm().set_jit_config(45, parseInt(process.env.HI, 16));
});
const hash = () => { const v = emulator.v86.cpu.devices.vga; let h = 0; for(const p of [v.plane0, v.plane1, v.plane2, v.plane3]) for(let i = 0; i < p.length; i += 7) h = (h * 31 + p[i]) | 0; return h; };
const samples = [];
const total = +process.env.SECS || 30;
const iv = setInterval(() => samples.push(hash()), 2000);
setTimeout(() => {
    clearInterval(iv);
    const tail = samples.slice(Math.floor(samples.length / 2));
    const changed = new Set(tail).size;
    const g = i => wm().get_jit_config(i) >>> 0;
    const rips = []; for(let i = 0; i < Math.min(g(5), 64); i++) rips.push(g(4000 + i).toString(16));
    if(process.env.VERIFY) { const n = g(5000), m = g(5001); console.log("verified blocks", n, "mismatches", m, "skipped(str)", g(5063)); for(let k = 0; k < Math.min(m, 4); k++) { const o = 5002 + k * 8; const v = i => g(o + i).toString(16); console.log(`  block ${v(1)}:${v(0)} n=${g(o+2)} what=${g(o+3)} jit=${v(4)} interp=${v(5)} flags jit=${v(6)} interp=${v(7)}`); } if(m) console.log("  mem mismatch info: addr_hi", g(5060).toString(16), "jit value lo/hi", g(5061).toString(16), g(5062).toString(16)); }
    console.log(`${process.env.LO || "-"}..${process.env.HI || "-"} mask=${process.env.JIT64_MASK || 0}: ${changed > 1 ? "RUNNING" : "FROZEN"} (distinct screens ${changed}/${tail.length}) compiled=${g(5)} blocks:${process.env.SHOWRIPS ? rips.join(" ") : ""}`);
    process.exit(0);
}, total * 1000);
