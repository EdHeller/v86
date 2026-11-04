#!/usr/bin/env python3
# The reference list of x86 encodings for tools/x86_coverage: every opcode of the one-byte, 0F, 0F 38 and 0F 3A maps
# (and x87), with no prefix, 66, F2 or F3, with each ModRM reg field in a register form (mod 11, every rm) and a memory
# form (mod 00, rm 0, or rm 5 / rip-relative avoided), decoded by GNU objdump in 16-, 32- and 64-bit code.
# No VEX/EVEX/XOP (v86 has no AVX). Writes tools/x86_coverage/reference-<bits>.json:
#   [{ "bytes": "660f3800c1", "full": "660f3800c1", "mnemonic": "pshufb", "text": "pshufb xmm0,xmm1", "map": "0f38",
#      "form": "reg" }, ...]                (full: the whole instruction, its immediates are 0xCC bytes)
#
#   python3 tools/x86_coverage/reference.py

import json, os, re, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
SLOT = 32
PAD = 0xCC

PREFIX_BYTES = {0x26, 0x2E, 0x36, 0x3E, 0x64, 0x65, 0x66, 0x67, 0xF0, 0xF2, 0xF3, 0x0F}
# objdump's prefix words that are not the instruction itself
PREFIX_WORDS = {"data16", "data32", "addr16", "addr32", "lock", "rep", "repz", "repnz", "repe", "repne", "bnd",
                "notrack", "cs", "ds", "es", "ss", "fs", "gs", "rex", "rex.w", "rex.b", "rex.x", "rex.r", "rex.wb",
                "xacquire", "xrelease", "{vex}"}

def candidates(bits):
    out = []
    # (64-bit code: also REX.W, the 64-bit operand size, after each legacy prefix; and REX.RB for r8..r15)
    prefixes = [b"", b"\x66", b"\xf2", b"\xf3"]
    if bits == 64:
        prefixes += [b"\x48", b"\x66\x48", b"\xf2\x48", b"\xf3\x48", b"\x45", b"\x66\x45"]
    for pfx in prefixes:
        # one-byte map (x87 included): every ModRM in the register form, the memory form per reg field
        for op in range(256):
            if op in PREFIX_BYTES:
                continue
            if bits == 64 and 0x40 <= op <= 0x4F:
                continue  # REX
            if op in (0xC4, 0xC5, 0x62, 0x8F) and bits == 64:
                pass
            for reg in range(8):
                for rm in range(8):
                    out.append(("1", pfx + bytes([op, 0xC0 | reg << 3 | rm]), "reg"))
                out.append(("1", pfx + bytes([op, reg << 3]), "mem"))
        # 0F map
        for op in range(256):
            if op in (0x38, 0x3A):
                continue
            for reg in range(8):
                for rm in range(8):
                    out.append(("0f", pfx + bytes([0x0F, op, 0xC0 | reg << 3 | rm]), "reg"))
                out.append(("0f", pfx + bytes([0x0F, op, reg << 3]), "mem"))
        # 0F 38 and 0F 3A maps (an imm8 follows in 0F 3A: the padding supplies it)
        for esc in (0x38, 0x3A):
            for op in range(256):
                for reg in range(8):
                    out.append(("0f%02x" % esc, pfx + bytes([0x0F, esc, op, 0xC0 | reg << 3 | 1]), "reg"))
                    out.append(("0f%02x" % esc, pfx + bytes([0x0F, esc, op, reg << 3]), "mem"))
    # (the C4/C5/62/8F VEX/EVEX/XOP escapes decode as VEX in some forms: skipped by mnemonic below)
    return out

def decode(bits, cands):
    with tempfile.TemporaryDirectory() as d:
        blob = bytearray()
        for _, b, _ in cands:
            blob += b + bytes([PAD]) * (SLOT - len(b))
        p = os.path.join(d, "c.bin")
        open(p, "wb").write(blob)
        machine = {16: ["-m", "i8086"], 32: ["-m", "i386"], 64: ["-m", "i386:x86-64"]}[bits]
        txt = subprocess.run(["objdump", "-D", "-b", "binary", "--insn-width=16", *machine, "-M", "intel", p],
                             capture_output=True, text=True).stdout
    at = {}
    for line in txt.splitlines():
        m = re.match(r"\s*([0-9a-f]+):\t([0-9a-f ]+?)\s*\t(.*)$", line)
        if not m:
            continue
        addr = int(m.group(1), 16)
        if addr % SLOT == 0:
            at[addr // SLOT] = (m.group(2).replace(" ", ""), m.group(3).strip())
    result = []
    seen = set()
    for i, (mp, b, form) in enumerate(cands):
        if i not in at:
            continue
        hexbytes, text = at[i]
        words = text.split()
        while words and words[0] in PREFIX_WORDS:
            words = words[1:]
        if not words or words[0] in ("(bad)", "int3") or text.startswith("(bad)"):
            continue
        mnemonic = words[0]
        if mnemonic.startswith("v") and mnemonic not in ("verr", "verw"):
            continue  # VEX
        # the instruction's own bytes (without the padding it consumed as immediates)
        own = b.hex()
        key = (own, mnemonic)
        if key in seen:
            continue
        seen.add(key)
        # (the full instruction: with the immediates and displacements it took from the int3 padding)
        result.append({"bytes": own, "full": hexbytes, "mnemonic": mnemonic, "text": " ".join(words), "map": mp, "form": form})
    return result

for bits in (16, 32, 64):
    cands = candidates(bits)
    ref = decode(bits, cands)
    path = os.path.join(HERE, "reference-%d.json" % bits)
    json.dump(ref, open(path, "w"), indent=0)
    mn = sorted({r["mnemonic"] for r in ref})
    print("%d-bit: %d candidates, %d valid encodings, %d mnemonics -> %s" % (bits, len(cands), len(ref), len(mn), path))
