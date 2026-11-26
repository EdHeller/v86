# v86 (32-bit x86) benchmark kit: original vs Rust port

Two builds of the 32-bit core. Each folder has the files of one build, which belong together (the Rust build's
JavaScript expects the core's new imports and exports; mixing folders does not start):

- `libv86.js`: the library as a classic script (`<script src="libv86.js">` defines `window.V86`; under Node
  `require` it from a `.cjs` file or a folder without `"type": "module"`)
- `libv86.mjs`: the same as an ES module (`import { V86 } from "./libv86.mjs"`; used by bench.mjs)
- `v86.wasm`: the core
- `v86-fallback.wasm`: the same core built without SIMD and the newer WebAssembly features. libv86.js loads it
  when `v86.wasm` does not instantiate (older browsers): it looks for `v86-fallback.wasm` beside `v86.wasm`
  (`wasm_path` with `v86.wasm` replaced). Keep the matching one beside each core; the original build's fallback
  does not start with the Rust build's JavaScript.

The folders:

- `original/`: branch `Vibe-Claude` at b1aaf25, before the device port (the JavaScript devices).
- `rust/`: branch `Vibe-Claude-rust` at c18db79, with the devices in Rust.

`bios/` has the SeaBIOS and VGA BIOS both were tested with. `buildroot-bzimage68.bin` is the i686 buildroot image
(the no-GUI bzImage with its built-in rootfs, 10 MB) both scripts use by default: keep it in this folder, beside
bench.mjs and bench.html.

## Run it

    node bench.mjs --runs 3

Node 20 or newer. Each round boots the image once per build (taking turns), times the shell prompt and five guest
workloads (a shell loop, md5 and sha256 of zeros, gzip of random data, 500 forks), then prints medians and the
change of rust against original. `--builds rust` or `--builds original` runs one of them.

In a browser, use either folder's `libv86.mjs` as you use v86 now (`import { V86 } from "./rust/libv86.mjs"`,
`wasm_path: "rust/v86.wasm"`), with `bzimage: { url: ".../buildroot-bzimage68.bin" }` and
`cmdline: "tsc=reliable mitigations=off console=ttyS0"`.

## In a browser (Firefox, Safari, Chrome): bench.html

The same workloads on the browser's own engine. Serve this folder over HTTP and open the page:

    python3 -m http.server 8000      (in this folder; then http://localhost:8000/bench.html)

The page loads `buildroot-bzimage68.bin` from this folder (always over HTTP: opening bench.html as a file, or
choosing a file from elsewhere, can trip the browser's "may not load or link to file:///" check). Choose the builds
and the rounds, and Run. The "Experiments" switches turn on three 32-bit
JIT options for every build that lost in Chrome/V8 (block chaining, nested calls, bigger modules); another engine
may make the calls between generated modules cheaper. Results to compare: a run with no switch against a run with
one switch, same browser. Keep the tab in the foreground.

## What to expect

The Rust branch also has 32-bit JIT work (inline rotates, compiling page ends, page-crossing instructions and
calls, several modules compiled at once, sooner recompiles, more flag elision, conditional jumps computed at their
cmp/test), a faster interpreter for code that is not compiled (inlined decoding, TLB fast paths, decoded
instructions cached per page) and cheaper JIT bookkeeping. Measured here with bench.mjs (2026-11-18, 5 rounds,
host clock, medians):

| | original | rust | change |
|---|---|---|---|
| boot to shell | 2.50 s | 2.16 s | -13.6 % |
| shell loop | 10.07 s | 9.25 s | -8.1 % |
| md5 64 MiB | 1.28 s | 0.95 s | -25.4 % |
| sha256 32 MiB | 5.58 s | 1.95 s | -65.0 % |
| gzip 16 MiB | 4.99 s | 4.38 s | -12.2 % |
| fork 500 | 2.73 s | 2.60 s | -4.8 % |

Not in bench.mjs: FreeDOS from floppy reaches its prompt with a quarter of the CPU time (code marks without paging
were most of a DOS boot), and SeaBIOS no longer reads the VGA option ROM through JavaScript. Take several rounds;
your machine's numbers are the ones that count.

## From a script, in headless Firefox (no browser driver needed)

`bench.html` takes URL parameters: `builds=a,b` (a build written `dir#k=v+k=v` runs that folder's core with those
`set_jit_config` values, so one core can be compared with one setting changed: `new,new%2321=1`; several: `new%2321=1%2B30=0`, since + is a space in a URL), `rounds=N`,
`auto` (start at once) and `report=/log` (POST every log line there). `serve.mjs` serves the folder and appends the
posted lines to a file:

    node serve.mjs "$PWD" 8123 bench.log &
    firefox --headless --no-remote --profile /tmp/ffprofile \
        "http://127.0.0.1:8123/bench.html?builds=new,new%2321=1&rounds=4&auto&report=/log"

(Firefox from https://download.mozilla.org/?product=firefox-latest&os=linux64 runs unpacked. `bench.log` gets a
`round ...` line per run and `done` at the end.)

In this repository: the scripts only; put the builds' folders (libv86.mjs, v86.wasm), `bios/` and the image beside
them.
