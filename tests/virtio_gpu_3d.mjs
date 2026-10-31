#!/usr/bin/env node
// virtio-gpu's 3D on x86_64: Alpine 3.24 (the official virt ISO) in a headless Chromium with WebGPU (SwiftShader's
// Vulkan when there is no GPU), graphics_adapter "virtio_gpu" with the page's renderer (build/glbridge/libv86-webgpu.js).
// The guest installs Mesa's virgl driver (Alpine 3.23's Mesa, from the repository image of tools/alpine_repo.mjs),
// and the checks are: the driver sees +virgl, EGL on GBM reports the virgl renderer, kmscube renders frames with it,
// and the cube is on the scanout.
//
//   node tools/alpine_repo.mjs --arch x86_64        (once: build/x64-linux/gpu-repo.tar)
//   make build/libv86.js build/v86.wasm build/v86-virtio-gpu.js glbridge
//   node tests/virtio_gpu_3d.mjs                    (VGPU3D_SECS: the boot limit, default 600)

import fs from "node:fs";
import http from "node:http";
import path from "node:path";
import url from "node:url";
import { createRequire } from "node:module";

const root = path.join(url.fileURLToPath(new URL(".", import.meta.url)), "..");
const { iso_files } = await import(path.join(root, "src/arm/iso.js"));
const iso_path = path.join(root, "build/alpine-virt-3.24.3-x86_64.iso");
const repo = path.join(root, "build/x64-linux/gpu-repo.tar");
for(const file of [iso_path, repo, path.join(root, "build/libv86.js"), path.join(root, "build/v86-virtio-gpu.js"), path.join(root, "build/glbridge/libv86-webgpu.js")])
{
    if(!fs.existsSync(file)) throw new Error(file + " is missing (see the comment at the top)");
}
// (the kernel and initramfs out of the ISO, for the page)
const boot = path.join(root, "build/x64-linux/boot");
fs.mkdirSync(boot, { recursive: true });
{
    const b = fs.readFileSync(iso_path);
    const files = iso_files(b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength));
    for(const [name, file] of [["BOOT/VMLINUZ_VIRT", "vmlinuz-virt"], ["BOOT/INITRAMFS_VIRT", "initramfs-virt"]])
    {
        const [offset, length] = files.get(name);
        fs.writeFileSync(path.join(boot, file), b.subarray(offset, offset + length));
    }
}

// a static server of the repository (with ranges)
const TYPES = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".json": "application/json" };
const server = http.createServer((request, response) => {
    const file = path.join(root, decodeURIComponent(new URL(request.url, "http://x").pathname));
    if(!file.startsWith(root) || !fs.existsSync(file) || !fs.statSync(file).isFile()) { response.writeHead(404); response.end(); return; }
    const size = fs.statSync(file).size;
    const headers = { "Content-Type": TYPES[path.extname(file)] || "application/octet-stream", "Accept-Ranges": "bytes" };
    const range = /bytes=(\d+)-(\d*)/.exec(request.headers.range || "");
    if(range)
    {
        const start = +range[1], end = range[2] ? +range[2] : size - 1;
        response.writeHead(206, { ...headers, "Content-Range": `bytes ${start}-${end}/${size}`, "Content-Length": end - start + 1 });
        fs.createReadStream(file, { start, end }).pipe(response);
    }
    else
    {
        response.writeHead(200, { ...headers, "Content-Length": size });
        fs.createReadStream(file).pipe(response);
    }
});
await new Promise(resolve => server.listen(0, "127.0.0.1", resolve));
const port = server.address().port;

const { chromium } = createRequire(import.meta.url)("playwright");
const browser = await chromium.launch({ args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-webgpu-adapter=swiftshader"] });
const page = await browser.newPage();
page.on("pageerror", e => console.error("page error:", e.message));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const serial = () => page.evaluate(() => window.serial);
const wait_for = async (re, seconds, from = 0) => {
    const t0 = Date.now();
    for(;;)
    {
        const text = (await serial()).slice(from);
        if(re.test(text)) return text;
        if(Date.now() - t0 > seconds * 1000) throw new Error("timeout waiting for " + re + "\n" + text.slice(-1500));
        await sleep(500);
    }
};
// (a command, then a marker the shell computes, so that its echo on the console does not match: the output up to it,
// checked against `expect`)
let step = 0;
const run = async (command, expect, seconds = 120) => {
    const mark = (await serial()).length;
    const id = ++step;
    await page.evaluate(c => window.emulator.serial0_send(c + "\n"), command + "; echo STEP_DONE_$((" + id + "+1000))");
    const text = await wait_for(new RegExp("STEP_DONE_" + (id + 1000)), seconds, mark);
    if(expect && !expect.test(text)) throw new Error("`" + command + "` did not print " + expect + ":\n" + text.slice(-1500));
    return text;
};

const APK = "apk add --no-network --repository /mnt/repo/main --repository /mnt/repo/community";
const APK_VIRGL = "printf '@v323 /mnt/repo/v3.23/main\\n@v323 /mnt/repo/v3.23/community\\n' >> /etc/apk/repositories; " + APK +
    " mesa@v323 mesa-dri-gallium@v323 mesa-gbm@v323 mesa-egl@v323 mesa-gl@v323 mesa-gles@v323 llvm21-libs@v323";

try
{
    await page.goto(`http://127.0.0.1:${port}/tests/browser/virtio_gpu_3d.html`);
    const webgpu = await page.evaluate(async () => !!(navigator.gpu && await navigator.gpu.requestAdapter()));
    if(!webgpu) throw new Error("this Chromium has no WebGPU adapter");
    const t0 = Date.now();
    await wait_for(/localhost login:/, +process.env.VGPU3D_SECS || 600);
    console.log("login after %ss", ((Date.now() - t0) / 1000).toFixed(1));
    await page.evaluate(() => window.emulator.serial0_send("root\n"));
    await wait_for(/localhost:~# /, 60);
    const level = await page.evaluate(() => window.emulator.v86.cpu.devices.graphics_adapter.device["virtio_gpu"].level);
    console.log("device level:", level);
    await run("dmesg | grep -i -E 'drm.*features' | tail -2", /\+virgl/);
    await run("mkdir -p /mnt/repo && tar -xf /dev/sda -C /mnt/repo && echo REPO_$((1+1))", /REPO_2/, 600);
    await run(APK_VIRGL + " kmscube mesa-utils >/tmp/apk.log 2>&1; echo APK_RC=$?; grep -i -A3 error /tmp/apk.log | head", /APK_RC=0/, 1200);
    const egl = await run("eglinfo -p gbm > /tmp/egl.log 2>&1; grep -m2 -E 'OpenGL (core profile )?renderer' /tmp/egl.log || head -30 /tmp/egl.log", null, 300);
    console.log(egl.split("\n").filter(l => /renderer|rror|fail/i.test(l)).join("\n"));
    if(!/virgl/i.test(egl)) throw new Error("EGL's renderer is not virgl");
    const cube = await run("kmscube -c 120 2>&1 | grep -i -E 'renderer|Rendered'", null, 600);
    console.log(cube.trim().split("\n").filter(l => /renderer|Rendered/i.test(l)).join("\n"));
    if(!/renderer:\s*"?virgl/i.test(cube) || !/Rendered [1-9]\d* frames/.test(cube)) throw new Error("kmscube did not render with virgl");
    // (the cube on the screen: kmscube in the background, the page's canvas while it runs; the cube's faces are
    // saturated colors, the console is grey on black)
    // (in the foreground: the screenshot while it runs, then its end)
    const cube_mark = (await serial()).length;
    await page.evaluate(() => window.emulator.serial0_send("kmscube -c 300 > /tmp/cube.log 2>&1; echo CUBE_$((40+2))\n"));
    await sleep(12000);
    if(process.env.VGPU3D_DEBUG) console.log(await page.evaluate(() => {
        const gpu = window.emulator.v86.cpu.devices.graphics_adapter.device["virtio_gpu"];
        const s = gpu.scanouts[0], r = gpu.resources.get(s.resource_id);
        const canvases = [...document.querySelectorAll("canvas")].map(c => c.width + "x" + c.height + ":" + c.style.display);
        return JSON.stringify({ resource: s.resource_id, direct: s.direct, three_d: r && !!r.three_d, blob: r && !!r.blob,
            data: r && r.data && r.data.length, stats: gpu.stats, canvases, has_vga_shown: gpu.vga_shown });
    }));
    await page.screenshot({ path: path.join(root, "build/x64-linux/virtio_gpu_3d.png") });
    const colored = await page.evaluate(() => {
        const canvas = document.querySelector("#screen_container canvas");
        if(!canvas || canvas.style.display === "none") return -1;
        const copy = document.createElement("canvas");
        copy.width = canvas.width;
        copy.height = canvas.height;
        copy.getContext("2d").drawImage(canvas, 0, 0);
        const d = copy.getContext("2d").getImageData(0, 0, copy.width, copy.height).data;
        let n = 0;
        for(let i = 0; i < d.length; i += 4) if(Math.max(d[i], d[i + 1], d[i + 2]) - Math.min(d[i], d[i + 1], d[i + 2]) > 80) n++;
        return n;
    });
    await wait_for(/CUBE_42/, 600, cube_mark);
    console.log("colored pixels on the screen while kmscube runs: %d", colored);
    if(colored < 5000) throw new Error("the cube is not on the screen (" + colored + " colored pixels)");
    console.log("ok: virtio-gpu 3D (virgl on WebGPU), level %s", level);
    await browser.close();
    server.close();
    process.exit(0);
}
catch(error)
{
    console.error("FAIL", error.message);
    try { await page.screenshot({ path: path.join(root, "build/x64-linux/virtio_gpu_3d-fail.png") }); } catch(e) {}
    await browser.close();
    process.exit(1);
}
