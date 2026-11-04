# Packaged builds

| Name | Commit | Date | Contents |
|---|---|---|---|
| demo-2026-10-31 | 164e604e | 2026-10-31 00:23 UTC | `alpine-demo.tar.gz`: this build at `/`, v64 at `/v64/` |
| demo-2026-10-31b | 8e471d56 | 2026-10-31 08:27 UTC | `alpine-demo.tar.gz`: this build at `/`, v64 at `/v64/` |
| demo-2026-10-31c | cdc353ba | 2026-10-31 09:32 UTC | `alpine-demo.tar.gz`: this build at `/`, v64 at `/v64/`, upstream v86 315b2b0 (Oct 30, 2026 21:24 GMT, EdHeller/v86 master) at `/v86/` |
| demo-2026-10-31d | 7715ab7f | 2026-10-31 11:02 UTC | `alpine-demo.tar.gz`: as 31c, plus virtio-gpu (`?graphics_adapter=virtio_gpu`) and its 3D on WebGPU (`?gpu3d=1`, `gpu-repo.tar`; the cube steps in README.txt) |
| cube-2026-11-03 | 1bd2393a | 2026-11-03 06:02 UTC | `v86-cube.tar.gz`: x86_64 only, virtio-gpu 3D on WebGPU (`?gpu3d=1`, `gpu-repo.tar`), the cube's commands in README.txt |
| cube-2026-11-03b | 613dc8c2 | 2026-11-03 16:19 UTC | `v86-cube.tar.xz` (x86_64) and `v86-cube-x86.tar.xz` (32-bit): kmscube on virgl, coi-serviceworker, worker cores; `v86-release.tar.gz` (the website files) |
| cube-2026-11-04 | 9cf6cd9b+ | 2026-11-04 02:11 UTC | as cube-2026-11-03b, plus the CPU wakes when the GPU answers (kmscube was capped near 5 fps on real GPUs) |
| release-2026-11-04 | 7d89c947 | 2026-11-04 07:06 UTC | `v86-release.tar.gz`: the website files with the speed fix below |

## Speed note (2026-11-04)

Every package above except release-2026-11-04 is **slower than upstream v86**, by about 10-30 % on 32-bit guests
(a busybox shell loop on 32-bit Alpine). The cause came in with the cooperative multicore scheduler (80a8a3a6,
2026-10-28): the compiler stopped inlining the dispatcher of compiled code (`cycle_internal`) into the main loop.
Fixed in 6721d285 and 7d89c947. Measured side by side after the fix (100000-step shell loop, three alternating
rounds): ours 16.1 s, upstream v86 315b2b0 15.5 s, the v86 this branch started from (cc37f35c, 2026-10-18) 15.8 s,
within this host's noise.

- **The faster builds:** release-2026-11-04 and anything from 7d89c947 on; before the regression, builds up to
  e51b7ffb (2026-10-28, before multicore) and upstream v86 itself.
- **The slower builds:** 80a8a3a6 up to 7d89c947 exclusive, which is every package from demo-2026-10-31 through
  cube-2026-11-04.

