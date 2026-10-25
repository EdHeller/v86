#!/usr/bin/env node
// Runner for the x64 tests. Assembles each tests/x64/**/*.asm with nasm into a
// flat binary, runs it in a fresh node process (worker.js) and compares the
// outcome with the `; EXPECT:` header of the test.
//
//   pass   test must pass
//   xfail  test is known to fail (unimplemented feature / open issue). It
//          fails the run when it unexpectedly passes (XPASS), so the header
//          gets flipped to `pass` once the feature lands.
//
// Options / environment:
//   TEST_NAME=regex        only run matching tests (path relative to tests/x64)
//   --native               instead run tests marked `; NATIVE: yes` as Linux
//                          ELF64 binaries on the host, to validate expectations
//   --qemu                 run the flat binaries under qemu-system-x86_64 (TCG) as a
//                          reference implementation, to validate the tests
//                          themselves. All tests are expected to pass there, except
//                          the few marked `; QEMU: xfail` where qemu deviates from
//                          real hardware.
//   --show-xfail           print the failure reason of expected failures
//   MAX_PARALLEL_TESTS=n

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import url from "node:url";
import { execFile, spawn } from "node:child_process";
import { promisify } from "node:util";

const exec = promisify(execFile);
const __dirname = url.fileURLToPath(new URL(".", import.meta.url));
const BUILD_DIR = path.join(__dirname, "build");
const TEST_NAME = new RegExp(process.env.TEST_NAME || "", "i");
const MAX_PARALLEL = +process.env.MAX_PARALLEL_TESTS || Math.max(1, os.cpus().length);
const NATIVE = process.argv.includes("--native");
const QEMU = process.argv.includes("--qemu");
const SHOW_XFAIL = process.argv.includes("--show-xfail");

function find_tests(dir, out = [])
{
    for(const e of fs.readdirSync(dir, { withFileTypes: true }))
    {
        const p = path.join(dir, e.name);
        if(e.isDirectory()) { if(e.name !== "inc" && e.name !== "build" && e.name !== "manual") find_tests(p, out); }
        else if(e.name.endsWith(".asm") || e.name.endsWith(".js") && (dir.includes("issues") || dir.includes("/js"))) out.push(p);
    }
    return out.sort();
}

function parse_header(source)
{
    const get = (key, def) => {
        const m = new RegExp("^(?:;|//)\\s*" + key + ":\\s*(.*)$", "mi").exec(source);
        return m ? m[1].trim() : def;
    };
    return {
        expect: get("EXPECT", "pass").toLowerCase().split(/\s/)[0],
        issue: get("ISSUE", ""),
        desc: get("DESC", ""),
        native: get("NATIVE", "no").toLowerCase().startsWith("yes"),
        serial: get("SERIAL", "no").toLowerCase().startsWith("yes"),
        qemu_xfail: get("QEMU", "pass").toLowerCase().startsWith("xfail"),
    };
}

async function run_one(file)
{
    const name = path.relative(__dirname, file).replace(/\.(asm|js)$/, "");
    const is_js = file.endsWith(".js");
    const meta = parse_header(fs.readFileSync(file, "utf8"));
    const base = path.join(BUILD_DIR, name.replace(/\//g, "_"));
    const r = { name, meta, outcome: undefined, detail: "" };

    try
    {
        if(is_js)
        {
            // plain node scripts (API / filesystem level tests), pass = exit code 0
            if(NATIVE || QEMU) { r.outcome = "skip"; return r; }
            try
            {
                await exec(process.execPath, [file], { timeout: 30000 });
                r.passed = true;
            }
            catch(e)
            {
                r.passed = false;
                const msg = (e.stderr || e.message || "").toString().split("\n").filter(l => l.trim());
                r.detail = msg.find(l => /Error|assert/i.test(l)) || msg[0] || "";
            }
        }
        else if(NATIVE)
        {
            if(!meta.native) { r.outcome = "skip"; return r; }
            await exec("nasm", ["-f", "elf64", "-DNATIVE", "-i", __dirname + "/", file, "-o", base + ".o"]);
            await exec("ld", ["-o", base + ".elf", base + ".o"]);
            try
            {
                await exec(base + ".elf", [], { timeout: 10000 });
                r.passed = true;
            }
            catch(e)
            {
                r.passed = false;
                r.detail = "native exit code " + e.code + " (check id " + ((e.code & 0x7F) || "?") + ")";
            }
        }
        else if(QEMU)
        {
            await exec("nasm", ["-f", "bin", "-i", __dirname + "/", file, "-o", base + ".bin"]);
            const res = await run_qemu(base + ".bin");
            r.passed = res.status === "pass";
            if(!r.passed) r.detail = res.status === "fail" ? "check #" + res.id + " (0x" + res.id.toString(16) + ") failed (actual " + res.actual + ")" : res.status;
        }
        else
        {
            await exec("nasm", ["-f", "bin", "-i", __dirname + "/", file, "-o", base + ".bin"]);
            try
            {
                const { stdout } = await exec(process.execPath,
                    [path.join(__dirname, "worker.js"), base + ".bin"],
                    { timeout: 30000, maxBuffer: 1 << 24 });
                const res = JSON.parse(stdout.trim().split("\n").pop());
                r.passed = res.status === "pass";
                if(!r.passed)
                {
                    r.detail = res.status === "fail" ?
                        "check #" + res.id + " (0x" + res.id.toString(16) + ") failed: actual " + res.actual :
                        res.status + (res.exceptions.length ? " (exceptions: " + res.exceptions + ")" : "");
                }
            }
            catch(e)
            {
                r.passed = false;
                const msg = (e.stderr || e.message || "").toString().split("\n").filter(l => l.trim());
                r.detail = "crash: " + (msg.find(l => /Error|panic|unreachable|assert/i.test(l)) || msg[0] || "");
            }
        }
    }
    catch(e)
    {
        // assembling failed: always a real error, never an expected failure
        r.outcome = "error";
        r.detail = (e.stderr || e.message).toString().trim();
        return r;
    }

    if(QEMU && meta.qemu_xfail) r.outcome = r.passed ? "XPASS" : "xfail";
    else if(NATIVE || QEMU) r.outcome = r.passed ? "ok" : "FAIL";
    else if(meta.expect === "xfail") r.outcome = r.passed ? "XPASS" : "xfail";
    else r.outcome = r.passed ? "ok" : "FAIL";
    return r;
}

// qemu reference run: a 64KB "bios" whose reset vector jumps to 0000:7c00, with
// the test image placed there by the loader device. The result block is read back
// through the monitor.
async function run_qemu(bin)
{
    const bios = path.join(BUILD_DIR, "reset.bios");
    const img = Buffer.alloc(65536, 0xF4);
    Buffer.from([0xEA, 0x00, 0x7C, 0x00, 0x00]).copy(img, 0xFFF0);
    fs.writeFileSync(bios, img);
    const child = spawn("qemu-system-x86_64", [
        "-display", "none", "-monitor", "stdio", "-serial", "none", "-no-reboot", "-machine", "pc,accel=tcg",
        "-cpu", "max", "-m", "64", "-bios", bios, "-device", `loader,file=${bin},addr=0x7c00`,
    ], { stdio: ["pipe", "pipe", "ignore"] });
    let out = "";
    child.stdout.on("data", d => out += d);
    const poll = async () => {
        for(let i = 0; i < 60; i++)
        {
            await new Promise(r => setTimeout(r, 200));
            out = "";
            child.stdin.write("xp /8wx 0x600\n");
            await new Promise(r => setTimeout(r, 150));
            const m = /0000000000000600:((?: 0x[0-9a-f]{8}){4})/.exec(out);
            const words = m ? m[1].trim().split(" ").map(Number) : [];
            if(words[0] === 0x53534150) return { status: "pass" };
            if(words[0] === 0x4C494146) return { status: "fail", id: words[1], actual: "0x" + words[3].toString(16) + words[2].toString(16).padStart(8, "0") };
        }
        return { status: "timeout" };
    };
    const res = await poll();
    child.kill();
    return res;
}

fs.mkdirSync(BUILD_DIR, { recursive: true });
const files = find_tests(__dirname).filter(f => TEST_NAME.test(path.relative(__dirname, f)));
const results = [];
// wall-clock sensitive tests (`; SERIAL: yes`) run alone, after everything else
const is_serial = f => /^(?:;|\/\/)\s*SERIAL:\s*yes/mi.test(fs.readFileSync(f, "utf8"));
const parallel_files = files.filter(f => !is_serial(f));
const serial_files = files.filter(is_serial);
let next = 0;
await Promise.all(Array.from({ length: Math.min(MAX_PARALLEL, parallel_files.length) }, async () => {
    while(next < parallel_files.length) results.push(await run_one(parallel_files[next++]));
}));
for(const f of serial_files) results.push(await run_one(f));
results.sort((a, b) => a.name.localeCompare(b.name));

const count = {};
for(const r of results)
{
    count[r.outcome] = (count[r.outcome] || 0) + 1;
    const show = r.outcome === "FAIL" || r.outcome === "XPASS" || r.outcome === "error" ||
        (SHOW_XFAIL && r.outcome === "xfail");
    if(r.outcome === "skip") continue;
    const tag = r.meta.issue ? " [" + r.meta.issue + "]" : "";
    console.log(r.outcome.padEnd(5), r.name + tag + (show && r.detail ? "\n        " + r.detail.replace(/\n/g, "\n        ") : ""));
}
console.log("\n" + Object.entries(count).map(([k, v]) => v + " " + k).join(", "));
if((count.FAIL || 0) + (count.XPASS || 0) + (count.error || 0) > 0)
{
    if(count.XPASS) console.log("XPASS: test now passes, change its header to `; EXPECT: pass`");
    process.exit(1);
}
