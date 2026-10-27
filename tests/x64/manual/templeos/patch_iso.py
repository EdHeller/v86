"""Writes a copy of a TempleOS ISO whose Once.HC skips the install/tour prompts and runs a serial selftest.

With TOS_SUITE=dungen only the DunGen demo runs, with a "J <jiffies>" heartbeat every second.
With TOS_SUITE=1 it instead runs the whole OSTestSuite (Tour option 12, 171 programs) and prints
"TS <n>" on the serial port whenever the test number changes, then "SUITE DONE".

usage: TOS_ISO=TempleOS.ISO python3 patch_iso.py out.ISO
"""
import struct, sys
import os
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import tosfile
body = r'''U0 Ser(U8 *s)
{
  while (*s)
    OutU8(0x3F8,*s++);
}
U0 Poll()
{
  U8 b[32];
  I64 l=-1;
  while (1) {
    if (progress4!=l) {
      l=progress4;
      StrPrint(b,"TS %d\n",l);
      Ser(b);
    }
    Sleep(50);
  }
}
Spawn(&Poll,,"Poll");
#include "::/Misc/OSTestSuite";
Ser("SUITE DONE\n");
'''.encode() if os.environ.get('TOS_SUITE') == '1' else r'''U0 Ser(U8 *s)
{
  while (*s)
    OutU8(0x3F8,*s++);
}
U0 Beat()
{
  U8 b[48];
  while (1) {
    StrPrint(b,"J %d\n",cnts.jiffies);
    Ser(b);
    Sleep(1000);
  }
}
Spawn(&Beat,,"Beat");
Ser("DUNGEN START\n");
#include "::/Demo/Games/DunGen";
Ser("DUNGEN DONE\n");
'''.encode() if os.environ.get('TOS_SUITE') == 'dungen' else r'''U0 Ser(U8 *s)
{
  while (*s)
    OutU8(0x3F8,*s++);
}
U0 Tmp()
{
  U8 buf[128];
  "\nx86_64 selftest\n";
  StrPrint(buf,"TOS 1+2=%d fact10=%d\n",1+2,10*9*8*7*6*5*4*3*2);
  "%s",buf;
  Ser(buf);
  StrPrint(buf,"sqrt2=%.6f sin1=%.6f\n",Sqrt(2.0),Sin(1.0));
  "%s",buf;
  Ser(buf);
  Ser("SELFTEST DONE\n");
}
Tmp;
'''.encode()
d = bytearray(tosfile.d)
for i, clus, size in tosfile.find('Once.HC.Z'):
    assert len(body) + 17 <= size, (len(body), size)
    hdr = struct.pack('<QQB', 17 + len(body), len(body), 1)
    d[clus*512: clus*512 + 17 + len(body)] = hdr + body
    struct.pack_into('<Q', d, i + 46, 17 + len(body))
open(sys.argv[1], 'wb').write(d)
print('patched', len(body))
