import struct, sys
def expand(data):
    csize, esize, ctype = struct.unpack_from("<QQB", data, 0)
    if ctype == 1: return data[17:17+esize]
    min_bits = 7 if ctype == 2 else 8
    mine = 1 << min_bits
    ch = [0]*4096; base = [0]*4096; children = {}
    st = dict(free_idx=mine, next_bits=min_bits+1, cur_entry=0, next_entry=0, entry_used=True)
    st['free_limit'] = 1 << st['next_bits']
    def get():
        if not st['entry_used']: return
        i = st['free_idx']; st['entry_used'] = False
        st['cur_entry'] = st['next_entry']; st['cur_bits'] = st['next_bits']
        if st['next_bits'] < 12:
            st['next_entry'] = i; i += 1
            if i == st['free_limit']:
                st['next_bits'] += 1; st['free_limit'] = 1 << st['next_bits']
        else:
            while True:
                i += 1
                if i == st['free_limit']: i = mine
                if not children.get(i): break
            st['next_entry'] = i
            p = base[i]
            if i in children.get(p, set()): children[p].discard(i)
        st['free_idx'] = i
    get(); st['entry_used'] = True
    src_pos = 17*8; src_size = csize*8
    bits = int.from_bytes(data[:csize], 'little')
    def rd(n):
        nonlocal src_pos
        v = (bits >> src_pos) & ((1 << n) - 1); src_pos += n; return v
    out = bytearray()
    lastcode = rd(st['next_bits']); out.append(lastcode & 0xFF)
    get(); last_ch = lastcode
    while len(out) < esize and src_pos + st['next_bits'] <= src_size:
        basecode = rd(st['next_bits'])
        stack = []
        if st['cur_entry'] == basecode:
            stack.append(last_ch); code = lastcode
        else: code = basecode
        while code >= mine:
            stack.append(ch[code]); code = base[code]
        stack.append(code); last_ch = code
        st['entry_used'] = True
        t = st['cur_entry']; base[t] = lastcode; ch[t] = last_ch
        children.setdefault(lastcode, set()).add(t)
        get()
        out.extend(reversed(stack))
        lastcode = basecode
    return bytes(out[:esize])
if __name__ == "__main__":
    import pycdlib
    iso = pycdlib.PyCdlib(); iso.open(sys.argv[1])
    path = sys.argv[2]
    from io import BytesIO
    b = BytesIO(); iso.get_file_from_iso_fp(b, iso_path=path)
    d = b.getvalue()
    sys.stdout.buffer.write(expand(d) if path.upper().endswith(".Z;1") or path.upper().endswith(".Z") else d)
