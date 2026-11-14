import sys, os, struct
sys.path.insert(0, 'docs/research/sharc-seq')
import sim_run
sys.path.insert(0, sim_run.unpack_wheel())
os.environ['PATH'] = os.path.join(sim_run.cces_home(), 'System') + os.pathsep + os.environ['PATH']
os.environ.setdefault('CCES_HOME', sim_run.cces_home())
import TpsdkServer.TpsdkServer as T
T.Connect('ChipFactory Simulator', 'ADSP-21571 Cycle-Accurate-Sim', 'ADSP-21571')
mts = T.GetMemoryTypes(0)
for m in list(mts)[58:]:
    print(m.MemoryId, m.Name, m.Width, flush=True)
def segs(i):
    m = mts[i]
    for s in m.MemorySegments:
        print('  seg', i, [(a, getattr(s, a)) for a in dir(s) if not a.startswith('_') and a not in ('this','thisown')], flush=True)
for i in (0, 39, 43, 5):
    print('seg of', i, mts[i].Name, flush=True); segs(i)

def rd(mid, ad, n=1, w=4):
    b = bytearray(n * w)
    try:
        T.GetMemory(0, mid, ad, n, 1, b); return bytes(b).hex()
    except Exception as e: return 'err ' + str(e)[-60:]
def wr(mid, ad, data, n=1):
    try:
        T.SetMemory(0, mid, ad, n, 1, bytearray(data)); return 'ok'
    except Exception as e: return 'err ' + str(e)[-60:]
W = 0x20004000 // 4
print('write NW', wr(39, W, struct.pack('>I', 0x12345678)), flush=True)
print('rd NW', rd(39, W), 'BW bytes', [rd(43, 0x20004000 + k, 1, 1) for k in range(4)], flush=True)
print('rd NW at W+1', rd(39, W + 1), flush=True)
print('SW 16-bit view (34)', [rd(34, 0x20004000 // 2 * 2 + k, 1, 2) for k in range(0, 4)], [rd(34, 0x10002000 + k, 1, 2) for k in range(3)], flush=True)

bw = lambda a, n=4: [rd(43, a + k, 1, 1) for k in range(n)]
print('BW write 20004001=AB', wr(43, 0x20004001, bytes([0xAB])), bw(0x20004000), flush=True)
print('top: NW write', wr(39, 0x200FFFFC // 4, struct.pack('>I', 0xCAFEF00D)), 'BW', bw(0x200FFFFC), flush=True)
print('BW write above end 20100000', wr(43, 0x20100000, bytes([0x5A])), rd(43, 0x20100000, 1, 1), flush=True)
print('BW write 200FFFFF', wr(43, 0x200FFFFF, bytes([0x77])), rd(43, 0x200FFFFF, 1, 1), flush=True)
print('BW 20000000 untouched', rd(43, 0x20000000, 1, 1), flush=True)
print('END', flush=True)
