# Open copy/v86 issues and their tests

Snapshot of the open issues of https://github.com/copy/v86 (112 issues, taken 2026-10-25) and where each one is covered.
Paths are relative to `tests/x64/`. A test marked `EXPECT: xfail` documents a still-open problem; it flips to
`XPASS` (and fails the run) as soon as the problem is fixed, at which point its header is changed to `EXPECT: pass`.

| Issue | Title | Coverage |
|---|---|---|
| [#1653](https://github.com/copy/v86/issues/1653) | Xvesa 24bbp incorrect info? | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1645](https://github.com/copy/v86/issues/1645) | [BUG] Keyboard doesn't work unless the mouse pointer has been locked before emulator is started | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#1635](https://github.com/copy/v86/issues/1635) | Missing PAM write protection corrupts the VGA BIOS during Windows 98 shutdown | **test**: issues/pam_write_protect_1635.asm |
| [#1633](https://github.com/copy/v86/issues/1633) | Delay floppy READ ID completion | not covered: device behaviour that needs a purpose-built guest program (candidate for `tests/devices`) |
| [#1631](https://github.com/copy/v86/issues/1631) | Inhibit interrupts after MOV SS and POP SS | **test**: issues/mov_ss_interrupt_shadow_1631.asm |
| [#1614](https://github.com/copy/v86/issues/1614) | PIT IRQ0 / ELKS guest clock runs at approximately half speed on copy.sh/v86 | **test**: issues/pit_irq0_rate_1614.asm (passes headless; the slowdown is browser/timing dependent) |
| [#1612](https://github.com/copy/v86/issues/1612) | How do you add two CDRom drives in html? | not covered: documentation / question, nothing to assert |
| [#1604](https://github.com/copy/v86/issues/1604) | Suggestion | not covered: documentation / question, nothing to assert |
| [#1572](https://github.com/copy/v86/issues/1572) | download from emulated system WITHOUT use floppy image | not covered: feature request without a precise, testable spec yet |
| [#1556](https://github.com/copy/v86/issues/1556) | Windows 9x CD-ROM device support fails with disks >= 256 MB | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1539](https://github.com/copy/v86/issues/1539) | OS support request Amithlon (2001-2005) | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1521](https://github.com/copy/v86/issues/1521) | MIDI works on Windows 95 only even times | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1511](https://github.com/copy/v86/issues/1511) | [Bug] MS DOS Prompt keeps "S>" with the upper right in Windows ME in v86 | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1504](https://github.com/copy/v86/issues/1504) | LILO can't load the second stage bootloader (DLX Linux) | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1496](https://github.com/copy/v86/issues/1496) | cdrom works initially but stops functioning after several minutes | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1451](https://github.com/copy/v86/issues/1451) | 9P Filesystem Performance Issue | not covered: performance issue; belongs in `tests/benchmark` |
| [#1444](https://github.com/copy/v86/issues/1444) | No documentation for splitting disk images into chunks | not covered: documentation / question, nothing to assert |
| [#1436](https://github.com/copy/v86/issues/1436) | Restart failure (NT 5) | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1429](https://github.com/copy/v86/issues/1429) | Docs changes/ideas | not covered: documentation / question, nothing to assert |
| [#1417](https://github.com/copy/v86/issues/1417) | v86 cannot boot Darwin 1.4.1 | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1416](https://github.com/copy/v86/issues/1416) | XV6 hard crashes when low memory, instead of a graceful panic. | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1414](https://github.com/copy/v86/issues/1414) | v86 does not emulate 'chunky' VGA graphics mode. | not covered: device behaviour that needs a purpose-built guest program (candidate for `tests/devices`) |
| [#1413](https://github.com/copy/v86/issues/1413) | XV6 fails with "ide disk 1 not present". (Does v86 emulate IDE?) | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1406](https://github.com/copy/v86/issues/1406) | Pasting strings into V86 with support for non-US keyboards | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#1383](https://github.com/copy/v86/issues/1383) | Plan 9 doesn't detect cdrom | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1382](https://github.com/copy/v86/issues/1382) | Hurd fails to read cdrom: `start iso9660fs: hd2: irq timeout: status=0x50` | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1360](https://github.com/copy/v86/issues/1360) | https requests | not covered: feature request without a precise, testable spec yet |
| [#1359](https://github.com/copy/v86/issues/1359) | Windows 98 freezes on startup screen and continues loading forever | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1358](https://github.com/copy/v86/issues/1358) | Issues with Win2k3 | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1356](https://github.com/copy/v86/issues/1356) | Implement SSE alignment exceptions | **test**: issues/sse_alignment_1356.asm, issues/sse_alignment_arith_1356.asm |
| [#1353](https://github.com/copy/v86/issues/1353) | Bug - When creating an empty disk in the UI, the emulator seems to silently fail when the disk size is above 2046 MB | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#1349](https://github.com/copy/v86/issues/1349) | Async CDROM functionality broken when booting from HDD | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1342](https://github.com/copy/v86/issues/1342) | Any luck with directx on win2k or xp? | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1331](https://github.com/copy/v86/issues/1331) | Suggestion: Writable rootfs using IndexedDB | not covered: feature request without a precise, testable spec yet |
| [#1324](https://github.com/copy/v86/issues/1324) | 9p cache seems broken on alpine build | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1322](https://github.com/copy/v86/issues/1322) | Is it possible to run Blitz3D games or Blitz3D (itself) in Windows 95 or Windows 98 in here? | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1296](https://github.com/copy/v86/issues/1296) | We need more bootsectors | not covered: feature request without a precise, testable spec yet |
| [#1292](https://github.com/copy/v86/issues/1292) | KolibriOS cdrom hangs after boot | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#1263](https://github.com/copy/v86/issues/1263) | Feature request - 16KB+7C00 memory on IBM PC | not covered: feature request without a precise, testable spec yet |
| [#1260](https://github.com/copy/v86/issues/1260) | Support of Microsoft Serial Mouse on /dev/ttyS1 | not covered: device behaviour that needs a purpose-built guest program (candidate for `tests/devices`) |
| [#1253](https://github.com/copy/v86/issues/1253) | Fix 16-bit mode instruction pointer wrap-around | **test**: issues/ip_wraparound_1253.asm |
| [#1197](https://github.com/copy/v86/issues/1197) | tcpip.js networking examples / docs | not covered: documentation / question, nothing to assert |
| [#1181](https://github.com/copy/v86/issues/1181) | feat: Download Text Mode buffer | not covered: feature request without a precise, testable spec yet |
| [#1075](https://github.com/copy/v86/issues/1075) | Feature Request- ACPI Wakeup Call upon initializing VM from saved image state | not covered: feature request without a precise, testable spec yet |
| [#1038](https://github.com/copy/v86/issues/1038) | SSSE3 support | **test**: stubs/ssse3_pshufb_palignr.asm, stubs/ssse3_hadd_hsub.asm, stubs/ssse3_abs_sign_mulhrsw.asm |
| [#1013](https://github.com/copy/v86/issues/1013) | Suggestion - Fast floats | not covered: performance issue; belongs in `tests/benchmark` |
| [#1007](https://github.com/copy/v86/issues/1007) | Sound in ReactOS | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#993](https://github.com/copy/v86/issues/993) | Mechanism to communicate from emulated OS to API | not covered: feature request without a precise, testable spec yet |
| [#978](https://github.com/copy/v86/issues/978) | Unhandled exception 000D with Borland Turbo Compiler | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#975](https://github.com/copy/v86/issues/975) | Add Offline Support | not covered: feature request without a precise, testable spec yet |
| [#945](https://github.com/copy/v86/issues/945) | QEMU config for Windows 98 that will work with v86? | not covered: documentation / question, nothing to assert |
| [#940](https://github.com/copy/v86/issues/940) | Boot Visopsys | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#930](https://github.com/copy/v86/issues/930) | Experiencing issues when running Windows 8.0, 8.1, and 10. | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#924](https://github.com/copy/v86/issues/924) | Symmetric multi-processing (SMP) support | not covered: feature request without a precise, testable spec yet |
| [#904](https://github.com/copy/v86/issues/904) | Support qcow with qcow-rs | not covered: feature request without a precise, testable spec yet |
| [#893](https://github.com/copy/v86/issues/893) | DirectX 5 on Windows 95 | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#881](https://github.com/copy/v86/issues/881) | FS9P Deleted Files Remain in Memory | **test**: issues/fs_unlink_frees_data_881.js (passes at the FS level today) |
| [#870](https://github.com/copy/v86/issues/870) | UX/UI/URI improvements proposal | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#864](https://github.com/copy/v86/issues/864) | Fullscreen mode doesn't work on Safari | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#849](https://github.com/copy/v86/issues/849) | Windows 98 startup/shutdown screen stretched too tall | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#823](https://github.com/copy/v86/issues/823) | Download bios and image files in parallel | not covered: feature request without a precise, testable spec yet |
| [#807](https://github.com/copy/v86/issues/807) | Windows 10 32-bit freezes after loading PE | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#790](https://github.com/copy/v86/issues/790) | Haiku R1 Beta 4 hangs during boot | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#762](https://github.com/copy/v86/issues/762) | Booting up vintage Hackintosh (Mac OS X) DVDs in v86 causes various kernel panics | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#759](https://github.com/copy/v86/issues/759) | Save disk changes in nodejs | not covered: feature request without a precise, testable spec yet |
| [#678](https://github.com/copy/v86/issues/678) | Support changing 9p mount tag at runtime | not covered: feature request without a precise, testable spec yet |
| [#659](https://github.com/copy/v86/issues/659) | read_file returns null for empty file | **test**: issues/fs_read_empty_file_659.js (passes at the FS level today) |
| [#652](https://github.com/copy/v86/issues/652) | Windows 2000: serial port not working | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#645](https://github.com/copy/v86/issues/645) | Windows NT 4.0 crashes after loading drivers | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#636](https://github.com/copy/v86/issues/636) | Guest reboots fail when using bzImage | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#626](https://github.com/copy/v86/issues/626) | Run a Linux binary under emulation | not covered: feature request without a precise, testable spec yet |
| [#620](https://github.com/copy/v86/issues/620) | MiniWindowsXP: Uncaught RangeError: Maximum call stack size exceeded | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#584](https://github.com/copy/v86/issues/584) | Touch screen issues | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#555](https://github.com/copy/v86/issues/555) | Windows 2000/98SE hang on boot. | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#534](https://github.com/copy/v86/issues/534) | Suggestion: run emulator in web worker | not covered: feature request without a precise, testable spec yet |
| [#530](https://github.com/copy/v86/issues/530) | Stream comunication between JavaScript and guest OS | not covered: feature request without a precise, testable spec yet |
| [#524](https://github.com/copy/v86/issues/524) | my Windows 3.1 Live CD is not working | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#519](https://github.com/copy/v86/issues/519) | Windows For Workgroups 3.11 Freezes after copying files | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#507](https://github.com/copy/v86/issues/507) | Windows ME stuck at black screen | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#502](https://github.com/copy/v86/issues/502) | Feature Request hooking memory addr/injecting memory/interrupts etc. (virtual hardware) | not covered: feature request without a precise, testable spec yet |
| [#473](https://github.com/copy/v86/issues/473) | Add e1000 network driver support for windows devices | not covered: feature request without a precise, testable spec yet |
| [#457](https://github.com/copy/v86/issues/457) | Alternative vgabios supported on V86 too | not covered: feature request without a precise, testable spec yet |
| [#433](https://github.com/copy/v86/issues/433) | Windows NT 4.0 NetLogon not works | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#420](https://github.com/copy/v86/issues/420) | Emulation Freezes when mixed content connection is made | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#405](https://github.com/copy/v86/issues/405) | Support more VIRTIO Devices | not covered: feature request without a precise, testable spec yet |
| [#396](https://github.com/copy/v86/issues/396) | Direct writes to local disk images | not covered: feature request without a precise, testable spec yet |
| [#393](https://github.com/copy/v86/issues/393) | Uncaught RangeError: Maximum call stack size exceeded | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#373](https://github.com/copy/v86/issues/373) | Webpack support | not covered: documentation / question, nothing to assert |
| [#343](https://github.com/copy/v86/issues/343) | Consider handling Intel's Control-Flow Enforcement Technology | **test**: issues/cet_endbr_nop_343.asm (passes today) |
| [#335](https://github.com/copy/v86/issues/335) | API to interact with guest OS Filesystem | not covered: feature request without a precise, testable spec yet |
| [#301](https://github.com/copy/v86/issues/301) | Boot Redox OS | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#288](https://github.com/copy/v86/issues/288) | Some animations not updating while playing sound in Windows Media Player | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#278](https://github.com/copy/v86/issues/278) | SIGSEGV when run a program in gdb | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#275](https://github.com/copy/v86/issues/275) | Double-click and dragging not working on iOS | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#263](https://github.com/copy/v86/issues/263) | Support for UEFI BIOS | not covered: feature request without a precise, testable spec yet |
| [#254](https://github.com/copy/v86/issues/254) | Mount zip as 9p filesystem | not covered: feature request without a precise, testable spec yet |
| [#250](https://github.com/copy/v86/issues/250) | Windows 98 fell into black screen after start screen | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#247](https://github.com/copy/v86/issues/247) | APIC capability | **test**: issues/apic_present_247.asm |
| [#227](https://github.com/copy/v86/issues/227) | When booting linux3.iso for example and entering full-screen mode output size is still small | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#200](https://github.com/copy/v86/issues/200) | electron as screen output for nodejs example | not covered: feature request without a precise, testable spec yet |
| [#194](https://github.com/copy/v86/issues/194) | [Feature Idea] Any plans to create a ChromeOS app on the Web Store? | not covered: feature request without a precise, testable spec yet |
| [#193](https://github.com/copy/v86/issues/193) | Windows 98/95 DOS prompt - cursor jumps around | not covered: needs a guest OS image that is not in the repository (use `tests/full` style tests once an image is available) |
| [#161](https://github.com/copy/v86/issues/161) | MS-DOS 6.22 freeze after executing "-t" (trace) command | **test**: issues/single_step_tf_103_161.asm |
| [#133](https://github.com/copy/v86/issues/133) | x86_64 support | **test**: `long/*.asm` (the long mode suite, starting with `long/00_enter_long_mode.asm`) |
| [#108](https://github.com/copy/v86/issues/108) | Build a debugger interface | not covered: feature request without a precise, testable spec yet |
| [#104](https://github.com/copy/v86/issues/104) | Add some way to handle interrupts/exceptions from JS | not covered: feature request without a precise, testable spec yet |
| [#103](https://github.com/copy/v86/issues/103) | Implement single stepping | **test**: issues/single_step_tf_103_161.asm |
| [#100](https://github.com/copy/v86/issues/100) | Any plan to write some tutorials to explain how v86's components work? | not covered: documentation / question, nothing to assert |
| [#99](https://github.com/copy/v86/issues/99) | keyboard layout ? | not covered: browser UI / input behaviour; not reachable from headless node tests |
| [#80](https://github.com/copy/v86/issues/80) | Graphical output for nodejs | not covered: feature request without a precise, testable spec yet |
| [#51](https://github.com/copy/v86/issues/51) | add virtio gpu support | not covered: feature request without a precise, testable spec yet |
| [#24](https://github.com/copy/v86/issues/24) | User interface | not covered: browser UI / input behaviour; not reachable from headless node tests |

13 of 112 issues have a test. The remaining ones are OS-compatibility reports, UI problems, documentation questions
or feature requests; they are listed so the gap is explicit, not forgotten.
