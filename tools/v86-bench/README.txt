v86 speed comparison (32-bit Alpine 3.24 on three builds of v86). Needs Node 18 or later.

  tar xJf v86-bench.tar.xz && cd v86-bench
  node run.mjs 3 | tee results.txt

3 = rounds (each round boots every build once; the order rotates). A round takes about 5 minutes per build here.
Close other programs while it runs. Send back results.txt.

  node run.mjs 1 ours          one build only, for a quick check

Builds (builds/*/VERSION.txt): base = the v86 this branch started from, upstream = upstream v86 master,
ours = Vibe-Claude. Tests: login (boot time), loop (shell loop of 100000 steps), gzip (8 MiB), sha (sha256 of
64 MiB), fork (500 processes). Wall-clock seconds; lower is better.

Making the package: builds/<name>/ holds libv86.mjs and v86.wasm of each tree (make build/libv86.mjs
build/v86.wasm; base is commit cc37f35c, upstream 315b2b0) and a VERSION.txt; bios/ has seabios.bin and
vgabios.bin; images/ has vmlinuz-x86 and initramfs-x86 (from the ISO's boot/) and alpine-virt-3.24.3-x86.iso.
