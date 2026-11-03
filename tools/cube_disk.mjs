#!/usr/bin/env node
// The cube demo's disk (hda): an ext2 file system that Alpine's live boot finds by itself, with
//  - the package repositories of `tools/alpine_repo.mjs --set cube` (Alpine 3.24 with alpine-base and kmscube,
//    and under v3.23/ Alpine 3.23's Mesa, which has the virgl driver), each marked .boot_repository, and
//  - cube.apkovl.tar.gz, whose /etc/apk/world asks for Mesa below 26 (3.23's) and kmscube.
// Alpine's init installs that world into the root file system at boot, before the login: kmscube runs right away,
// no apk commands. The image is build/x64-linux/cube-disk.img (build/x86-linux/ for --arch x86).
//
//   node tools/alpine_repo.mjs --arch x86_64 --set cube && node tools/cube_disk.mjs
//   node tools/alpine_repo.mjs --arch x86 --set cube && node tools/cube_disk.mjs --arch x86      (32-bit Alpine)

import fs from "node:fs";
import path from "node:path";
import url from "node:url";
import { spawnSync } from "node:child_process";

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), "..");
// --arch x86_64 (the default) or x86 (32-bit Alpine)
const ARCH = process.argv.includes("--arch") ? process.argv[process.argv.indexOf("--arch") + 1] : "x86_64";
if(!["x86_64", "x86"].includes(ARCH)) throw new Error("--arch: x86_64 or x86");
const DIR = path.join(ROOT, ARCH === "x86" ? "build/x86-linux" : "build/x64-linux");
const repo = path.join(DIR, "cube-repo");
const out = path.join(DIR, "cube-disk.img");
const stage = path.join(DIR, "cube-disk");
if(!fs.existsSync(path.join(repo, "v3.23"))) throw new Error(repo + " is missing: node tools/alpine_repo.mjs --arch x86_64 --set cube");

const WORLD = [
    "alpine-base", "openssl", "ssl_client",
    // (Alpine 3.24's Mesa 26 has no virgl driver: 3.23's 25.2)
    "mesa<26", "mesa-dri-gallium<26", "mesa-gbm<26", "mesa-egl<26", "mesa-gl<26", "mesa-gles<26",
    "kmscube",
];

// 3.23's packages that 3.24's directories do not have (llvm21-libs, ...): pinned to the version here, or apk would
// take a newer one that 3.24's index lists but the repository does not hold
const names = dir => fs.existsSync(dir) ? fs.readdirSync(dir).filter(f => f.endsWith(".apk")) : [];
const split = file => { const m = /^(.+)-([^-]+-r\d+)\.apk$/.exec(file); return m ? [m[1], m[2]] : null; };
const have_324 = new Set(["main", "community"].flatMap(r => names(path.join(repo, r, ARCH))).map(f => split(f)?.[0]));
for(const file of ["main", "community"].flatMap(r => names(path.join(repo, "v3.23", r, ARCH))))
{
    const [name, version] = split(file) || [];
    if(name && !have_324.has(name) && !WORLD.some(w => w.startsWith(name + "<"))) WORLD.push(name + "=" + version);
}

const run = (program, args, options) => {
    const result = spawnSync(program, args, { encoding: "utf8", ...options });
    if(result.status !== 0) throw new Error(program + ": " + (result.stderr || result.error));
};

fs.rmSync(stage, { recursive: true, force: true });
fs.mkdirSync(stage, { recursive: true });
for(const dir of ["main", "community", "v3.23/main", "v3.23/community"])
{
    fs.cpSync(path.join(repo, dir), path.join(stage, dir), { recursive: true });
    fs.writeFileSync(path.join(stage, dir, ".boot_repository"), "");
}

// the apkovl: the world, the host name of the live system, and the live system's default services
const ovl = path.join(DIR, "cube-ovl");
fs.rmSync(ovl, { recursive: true, force: true });
fs.mkdirSync(path.join(ovl, "etc/apk"), { recursive: true });
fs.writeFileSync(path.join(ovl, "etc/apk/world"), WORLD.join("\n") + "\n");
fs.writeFileSync(path.join(ovl, "etc/hostname"), "localhost\n");
fs.writeFileSync(path.join(ovl, "etc/.default_boot_services"), "");
fs.writeFileSync(path.join(ovl, "etc/motd"), "\nWelcome to Alpine! The spinning cube: type kmscube (Ctrl+C stops it).\n\n");
run("tar", ["--owner=0", "--group=0", "-czf", path.join(stage, "cube.apkovl.tar.gz"), "-C", ovl, "etc"]);

// an ext2 image of it, with some room
let bytes = 0;
const walk = dir => { for(const e of fs.readdirSync(dir, { withFileTypes: true })) { const p = path.join(dir, e.name); if(e.isDirectory()) walk(p); else bytes += fs.statSync(p).size; } };
walk(stage);
const mib = Math.ceil(bytes / (1 << 20) * 1.15) + 16;
fs.rmSync(out, { force: true });
run("mke2fs", ["-q", "-t", "ext2", "-L", "cube", "-E", "root_owner=0:0", "-d", stage, out, mib + "M"]);
console.log("Wrote " + path.relative(ROOT, out) + " (" + mib + " MiB)");
