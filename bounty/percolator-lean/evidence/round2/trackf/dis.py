import sys, struct

OPS = {}
def name_op(cls, code, src):
    alu = {0x0:'add',0x1:'sub',0x2:'mul',0x3:'div',0x4:'or',0x5:'and',0x6:'lsh',0x7:'rsh',0x8:'neg',0x9:'mod',0xa:'xor',0xb:'mov',0xc:'arsh'}
    jmp = {0x0:'ja',0x1:'jeq',0x2:'jgt',0x3:'jge',0x4:'jset',0x5:'jne',0x6:'jsgt',0x7:'jsge',0x8:'call',0x9:'exit',0xa:'jlt',0xb:'jle',0xc:'jslt',0xd:'jsle'}
    if cls in (4,7):
        n = alu.get(code,'alu%x'%code)
        return n + ('32' if cls==4 else '') + ('_x' if src else '_k')
    if cls in (5,6):
        if code==8: return 'call'
        if code==9: return 'exit'
        return jmp.get(code,'jmp%x'%code) + ('32' if cls==6 else '') + ('_x' if src else '_k')
    return None

def dis(path, start=0, count=None):
    data = open(path,'rb').read()
    # .text at file offset 0xb0 (from readelf: addr 0xb0, offset 0xb0)
    text_off = 0xb0
    text = data[text_off:]
    n = len(text)//8
    out = []
    for i in range(n):
        ins = text[i*8:(i+1)*8]
        op, regs, off, imm = ins[0], ins[1], struct.unpack_from('<h', ins, 2)[0], struct.unpack_from('<i', ins, 4)[0]
        dst, src = regs & 0xf, regs >> 4
        cls = op & 0x07; code = (op & 0xf0) >> 4; srcbit = (op & 0x08) >> 3
        pc = text_off + i*8
        if cls == 0:  # LD
            if op == 0x18: s = f"lddw r{dst}, {imm}"
            else: s = f"ld op={op:#x} r{dst}, {imm}"
        elif cls == 1: s = f"ldx{'b' if op==0x71 else 'h' if op==0x69 else 'w' if op==0x61 else 'dw'} r{dst}, [r{src}{off:+d}]"
        elif cls == 2: s = f"st{'b' if op==0x72 else 'h' if op==0x6a else 'w' if op==0x62 else 'dw'} [r{dst}{off:+d}], {imm}"
        elif cls == 3: s = f"stx{'b' if op==0x73 else 'h' if op==0x6b else 'w' if op==0x63 else 'dw'} [r{dst}{off:+d}], r{src}"
        else:
            nm = name_op(cls, code, srcbit)
            if nm and nm.startswith('call'): s = f"call {imm} ({imm:#x})"
            elif nm == 'exit': s = "exit"
            elif nm and (nm.startswith('j') and nm not in ('ja',)):
                s = f"{nm} r{dst}, {('r'+str(src)) if srcbit else imm}, {off:+d} -> {pc + 8 + off*8:#x}"
            elif nm == 'ja': s = f"ja {off:+d} -> {pc + 8 + off*8:#x}"
            elif nm:
                s = f"{nm} r{dst}, {('r'+str(src)) if srcbit else imm}"
            else: s = f"op={op:#x} r{dst} r{src} off={off} imm={imm}"
        out.append((pc, s))
    if count is None: return out
    return out[start:start+count]

if __name__ == '__main__':
    path = sys.argv[1]
    lo = int(sys.argv[2], 0) if len(sys.argv) > 2 else None
    hi = int(sys.argv[3], 0) if len(sys.argv) > 3 else None
    for pc, s in dis(path):
        if lo is not None and not (lo <= pc < hi): continue
        print(f"{pc:#08x}: {s}")
