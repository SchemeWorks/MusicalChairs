import json,os,sys
sys.path.insert(0,os.path.dirname(__file__))
from b58 import dec,enc
def shortvec(n):
    out=bytearray()
    while True:
        b=n&0x7f; n>>=7
        if n: out.append(b|0x80)
        else: out.append(b); break
    return bytes(out)
def legacy_message(m):
    h=m['header']
    out=bytes([h['numRequiredSignatures'],h['numReadonlySignedAccounts'],h['numReadonlyUnsignedAccounts']])
    out+=shortvec(len(m['accountKeys']))+b''.join(dec(k) for k in m['accountKeys'])
    out+=dec(m['recentBlockhash'])
    out+=shortvec(len(m['instructions']))
    for ix in m['instructions']:
        d=dec(ix['data'])
        out+=bytes([ix['programIdIndex']])+shortvec(len(ix['accounts']))+bytes(ix['accounts'])+shortvec(len(d))+d
    return out
def wire_tx(t):
    sigs=t['signatures']
    return shortvec(len(sigs))+b''.join(dec(s) for s in sigs)+legacy_message(t['message'])
