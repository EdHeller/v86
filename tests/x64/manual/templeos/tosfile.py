import struct, sys
sys.path.insert(0, '.')
from tosz import expand
import os
d = open(os.environ.get('TOS_ISO', 'TempleOS.ISO'), 'rb').read()
def find(name):
    nb = name.encode()
    i = -1
    res = []
    while True:
        i = d.find(nb + b'\x00', i + 1)
        if i < 0: break
        clus, size = struct.unpack_from('<QQ', d, i + 38)
        if 0 < clus * 512 < len(d) and 0 < size < 1 << 26 and d[i-1] != 0x2f:
            res.append((i, clus, size))
    return res
if __name__ == '__main__':
    n = sys.argv[1]
    for i, clus, size in find(n):
        raw = d[clus * 512: clus * 512 + size]
        if n.endswith('.Z'):
            hdr = struct.unpack_from('<QQB', raw, 0)
            if hdr[0] != size: continue
            raw = expand(raw)
        sys.stdout.buffer.write(raw); break
