# What a compiler costs on the Alpine x86_64 guest

Measured on the host from the Alpine 3.24 x86_64 packages (gcc 15.2.0-r5), no guest run needed. The Buildroot figure of
14 MiB in `next-session.md` is a zstd-compressed **RISC-V** rootfs, so it is not a target for x86: RISC-V code is denser
than x86_64 for this kind of binary only in part, and that build also had no C++, LTO, plugins or NLS. Compare variants
of our own system, not that number.

## Packages (APKINDEX; installed size / download size)

| Package | Installed | Download |
|---|---|---|
| gcc | 167.7 MB | 60.5 MB |
| g++ | 47.2 MB | 18.5 MB |
| libstdc++-dev | 23.0 MB | 4.2 MB |
| binutils | 10.6 MB | 3.2 MB |
| musl-dev | 9.9 MB | 3.4 MB |
| libstdc++ | 2.8 MB | 1.0 MB |
| isl, mpfr, mpc, gmp (gcc runtime libs) | about 3.5 MB | about 1.5 MB |
| tcc + tcc-libs + tcc-dev (community) | 0.5 MB | 0.2 MB |

`build-base` pulls in `gcc g++ binutils make libc-dev fortify-headers patch file`.

## Where the bytes go

Big single files in `gcc` and `g++` (each tens of MB): `cc1` 42.5 MB (C), `cc1plus` 45.0 MB (C++),
`lto1` 41.0 MB, `lto-dump` 41.0 MB (a second copy of the LTO compiler), plus the `plugin/` tree 11.4 MB and the
sanitizer archives (libasan/libtsan/libhwasan, several MB each). The unpacked `gcc` + `g++` payload is 214.9 MB.

## Variants (files from the gcc apk, host-side)

| Variant | Installed | tar + xz -9e |
|---|---|---|
| gcc + g++ (as shipped, unpacked payload) | 214.9 MB | 34.6 MB |
| C only: no `lto1`, `lto-dump`, plugins, sanitizers, share/ | 58.8 MB | 13.1 MB |

Removing `g++` + `libstdc++-dev` alone saves about 70 MB installed. `cc1` is already stripped, so `strip` gains nothing.
Still to try: drop man/info/locale (`usr/share` is already out in the C-only row), `tcc` as a 0.5 MB alternative compiler,
squashfs+xz or a separate lazily mounted toolchain image/9p share, and the musl-dev headers/libs actually needed.
No zstd binary was available here; the xz figures are the comparable ones.

## Follow-up: minimal C-only set (measured on the host, not run in a guest)

Why it is large: `cc1` (42.5 MB) is a monolithic, already-stripped compiler; the gcc apk ships the same 41 MB LTO
compiler twice (`lto1` and `bin/lto-dump`), plus 84 MB of `libexec/gcc`, 11 MB of plugin headers and the sanitizer
libs. `g++` adds another 45 MB `cc1plus` and `libstdc++-dev`. None of this is needed to compile C.

Smallest set that should build C programs: `gcc` driver, `cc1`, `collect2`, gcc's `include/` and `crt*.o`/`libgcc_eh.a`,
binutils `as ld ar`, and `musl-dev` (headers + libs):

| Component | Installed |
|---|---|
| `libexec` (cc1 + collect2) | 42 MB |
| `usr/lib` (musl libs + gcc crt/include) | 13 MB |
| `usr/bin` (gcc, as, ld, ar) | 6 MB |
| musl headers | 1.1 MB |
| **Total** | **62 MB installed, 15.6 MB tar+xz -9e** |

So against `build-base` (about 265 MB installed) a C-only toolchain is roughly 4x smaller installed, and about 15 MB
compressed. Almost all of what remains is `cc1`; going lower needs a different compiler (`tcc`, 0.5 MB) or serving
the toolchain as a separate, optional lazily loaded image or 9p share so the base image stays small.
Not verified: that this exact file set compiles and links in the guest (needs a guest run), and zstd/squashfs sizes.
