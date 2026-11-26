#!/usr/bin/env python3
"""Minimal Solana sBPF (eBPF) disassembler for static audit."""
import struct, sys

ALU = {0x00:'add',0x10:'sub',0x20:'mul',0x30:'div',0x40:'or',0x50:'and',0x60:'lsh',
       0x70:'rsh',0x80:'neg',0x90:'mod',0xa0:'xor',0xb0:'mov',0xc0:'arsh',0xd0:'end'}
JMP = {0x00:'ja',0x10:'jeq',0x20:'jgt',0x30:'jge',0x40:'jset',0x50:'jne',0x60:'jsgt',
       0x70:'jsge',0x80:'call',0x90:'callx',0xa0:'jlt',0xb0:'jle',0xc0:'jslt',0xd0:'jsle'}
SZ = {0x00:'w',0x08:'h',0x10:'b',0x18:'dw'}

def load_text(path):
    data = open(path,'rb').read()
    e_shoff = struct.unpack_from('<Q', data, 40)[0]
    e_shentsize, e_shnum, e_shstrndx = struct.unpack_from('<HHH', data, 58)
    secs = []
    for i in range(e_shnum):
        off = e_shoff + i*e_shentsize
        name, typ, flags, addr, offset, size = struct.unpack_from('<IIQQQQ', data, off)
        secs.append((name, typ, flags, addr, offset, size))
    strtab_off = secs[e_shstrndx][4]
    for (name, typ, flags, addr, offset, size) in secs:
        end = data.index(b'\x00', strtab_off+name)
        nm = data[strtab_off+name:end].decode()
        if nm == '.text':
            return data[offset:offset+size], addr
    raise SystemExit('no .text')

def decode(text):
    """Return list of (pc, opcode, dst, src, off, imm, raw)."""
    insns = []
    n = len(text)//8
    for pc in range(n):
        op, regs, off, imm = struct.unpack_from('<BBhi', text, pc*8)
        dst, src = regs & 0xF, (regs >> 4) & 0xF
        insns.append((pc, op, dst, src, off, imm))
    return insns

def fmt(insns, lo, hi, base_vaddr=0xb0):
    out = []
    i = lo
    while i < min(hi, len(insns)):
        pc, op, dst, src, off, imm = insns[i]
        cls = op & 0x07
        srct = 'r' if (op & 0x08) else 'i'
        a = f'{pc:5d} (0x{base_vaddr+pc*8:06x}): '
        if op == 0x18:  # lddw
            imm2 = insns[i+1][5] if i+1 < len(insns) else 0
            val = (imm & 0xFFFFFFFF) | ((imm2 & 0xFFFFFFFF) << 32)
            out.append(a + f'lddw r{dst}, 0x{val:x}')
            i += 2; continue
        if cls in (0x07, 0x04):  # ALU64/32
            opn = ALU.get(op & 0xF0, f'alu{op&0xf0:02x}')
            w = '32' if cls == 0x04 else ''
            if (op & 0xF0) == 0xD0:
                out.append(a + f'end{w} r{dst}, {imm}')
            elif (op & 0xF0) == 0x80:
                out.append(a + f'neg{w} r{dst}')
            else:
                rhs = f'r{src}' if srct == 'r' else f'{imm}'
                out.append(a + f'{opn}{w} r{dst}, {rhs}')
        elif cls == 0x01:  # LDX
            out.append(a + f'ldx{SZ.get(op&0x18,"?")} r{dst}, [r{src}{off:+d}]')
        elif cls == 0x02:  # ST
            out.append(a + f'st{SZ.get(op&0x18,"?")} [r{dst}{off:+d}], {imm}')
        elif cls == 0x03:  # STX
            out.append(a + f'stx{SZ.get(op&0x18,"?")} [r{dst}{off:+d}], r{src}')
        elif cls in (0x05, 0x06):  # JMP
            opn = JMP.get(op & 0xF0, f'jmp{op&0xf0:02x}')
            if op == 0x95:
                out.append(a + 'exit')
            elif opn == 'ja':
                out.append(a + f'ja {pc+off+1} ({off:+d})')
            elif opn == 'call':
                if src == 1:
                    out.append(a + f'call {pc+imm+1} (local, imm {imm})')
                elif src == 0:
                    out.append(a + f'call syscall 0x{imm & 0xFFFFFFFF:08x}')
                else:
                    out.append(a + f'call src={src} imm={imm}')
            else:
                rhs = f'r{src}' if srct == 'r' else f'{imm}'
                out.append(a + f'{opn} r{dst}, {rhs}, {pc+off+1} ({off:+d})')
        elif cls == 0x00:  # LD (non-dw)
            out.append(a + f'ld{SZ.get(op&0x18,"?")} r{dst}, {imm}')
        else:
            out.append(a + f'op{op:02x} r{dst} r{src} {off:+d} {imm}')
        i += 1
    return '\n'.join(out)

if __name__ == '__main__':
    text, vaddr = load_text(sys.argv[1])
    insns = decode(text)
    lo = int(sys.argv[2]) if len(sys.argv) > 2 else 0
    hi = int(sys.argv[3]) if len(sys.argv) > 3 else len(insns)
    print(fmt(insns, lo, hi, vaddr))
