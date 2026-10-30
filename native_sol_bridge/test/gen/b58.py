A='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def dec(s):
    n=0
    for c in s: n=n*58+A.index(c)
    b=n.to_bytes((n.bit_length()+7)//8,'big') if n else b''
    z=len(s)-len(s.lstrip('1'))
    return b'\x00'*z+b
def enc(b):
    n=int.from_bytes(b,'big'); out=''
    while n: n,r=divmod(n,58); out=A[r]+out
    z=len(b)-len(b.lstrip(b'\x00'))
    return '1'*z+out
