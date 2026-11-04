#!/usr/bin/env python3
"""String xref resolver for the KEF LSX firmware sections.

The DMP firmware is raw Thumb-2 with no container: no ELF header, no symbol
table, no load address. This disassembles a byte range with objdump and
resolves the addressing patterns the compiler actually uses, so you can find
which code references which strings.

Patterns handled:
  * add rX, pc, #imm   @ (adr rX, 0xTARGET)   <- the common one
  * ldr rX, [pc, #n]   @ (0xLIT) ; add rX, pc
  * movw/movt pairs (absolute)

Also infers the load base from `bl` targets, which cluster on the image base.

Usage:
  tools/xref.py s1.bin --base 0x400000 --range 0x400-0x3bfff [--xref 0x2c954]
                 [--map] [--region 0x2c860-0x2c9c0]
"""
import argparse, collections, re, struct, subprocess, sys, tempfile, os

OBJDUMP = "arm-none-eabi-objdump"
LINE = re.compile(r"\s*([0-9a-f]+):\s+([0-9a-f ]+)\t(.*)")


def disasm(path, start, end):
    out = subprocess.run(
        [OBJDUMP, "-D", "-b", "binary", "-m", "arm", "-M", "force-thumb",
         "--adjust-vma=0", f"--start-address={start}", f"--stop-address={end}", path],
        capture_output=True, text=True, check=True).stdout
    insns = []
    for ln in out.splitlines():
        m = LINE.match(ln)
        if m:
            insns.append((int(m.group(1), 16), m.group(3).strip()))
    return insns


def infer_base(insns):
    """`bl` targets cluster on the image base; pick the dominant high part."""
    vals = [int(m.group(1), 16) for _, t in insns
            for m in [re.match(r"bl\s+(?:0x)?([0-9a-f]+)", t)] if m]
    if not vals:
        return None, 0
    top = collections.Counter((v >> 20) << 20 for v in vals).most_common(1)[0]
    return top[0], top[1]


def resolve(insns, data):
    """Yield (code_addr, target_offset, kind)."""
    lit = {}
    for a, t in insns:
        m = re.match(r"ldr\s+(r\d+),\s*\[pc,\s*#\d+\]\s*@\s*\((0x[0-9a-f]+)\)", t)
        if m:
            lit[m.group(1)] = int(m.group(2), 16)
            continue
        m = re.search(r"adr\s+(r\d+),\s*(0x[0-9a-f]+)", t)
        if m:
            yield a, int(m.group(2), 16), "adr"
            continue
        m = re.match(r"add\s+(r\d+),\s*pc,\s*#\d+", t)
        if m and m.group(1) in lit:
            la = lit.pop(m.group(1))
            if la + 4 <= len(data):
                yield a, struct.unpack_from("<I", data, la)[0] + ((a + 4) & ~3), "lit+pc"
            continue
        m = re.match(r"movw\s+(r\d+),\s*#(0x[0-9a-f]+|\d+)", t)
        if m:
            lit["mw_" + m.group(1)] = (a, int(m.group(2), 0))
            continue
        m = re.match(r"movt\s+(r\d+),\s*#(0x[0-9a-f]+|\d+)", t)
        if m and ("mw_" + m.group(1)) in lit:
            lo = lit.pop("mw_" + m.group(1))
            yield a, (int(m.group(2), 0) << 16) | lo[1], "movw/movt"


def cstr(data, off, limit=120):
    if off >= len(data):
        return None
    end = data.find(b"\x00", off, off + limit)
    if end < 0:
        return None
    raw = data[off:end]
    if len(raw) < 3 or not all(0x20 <= b < 0x7f or b in (9, 10) for b in raw):
        return None
    return raw.decode("ascii")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("binary")
    ap.add_argument("--base", type=lambda s: int(s, 0), default=None,
                    help="load base; inferred from bl targets if omitted")
    ap.add_argument("--range", default="0x400-0x3bfff")
    ap.add_argument("--xref", type=lambda s: int(s, 0), action="append",
                    help="report code referencing this file offset")
    ap.add_argument("--map", action="store_true", help="list every string xref found")
    ap.add_argument("--region", help="disassemble and print hexStart-hexEnd")
    args = ap.parse_args()

    data = open(args.binary, "rb").read()
    lo, hi = (int(x, 0) for x in args.range.split("-"))
    insns = disasm(args.binary, lo, hi)
    print(f"# {len(insns)} instructions from {args.binary} [{hex(lo)}..{hex(hi)}]",
          file=sys.stderr)

    base, support = infer_base(insns)
    if args.base is None:
        print(f"# inferred load base: {hex(base) if base is not None else '?'} "
              f"({support} bl targets agree)", file=sys.stderr)
    else:
        base = args.base

    if args.region:
        rlo, rhi = (int(x, 0) for x in args.region.split("-"))
        for a, t in insns:
            if rlo <= a <= rhi:
                print(f"  {a:06x}  {t}")
        return

    pairs = list(resolve(insns, data))
    if args.map:
        rows = []
        for a, tgt, kind in pairs:
            s = cstr(data, tgt)
            if s:
                rows.append((a, tgt, s, kind))
        print(f"# {len(rows)} string xrefs")
        for a, tgt, s, kind in rows:
            print(f"  code 0x{a:05x} -> 0x{tgt:05x}  {s!r}   [{kind}]")
        return

    for want in args.xref or []:
        hits = [(a, k) for a, tgt, k in pairs if tgt == want]
        s = cstr(data, want)
        print(f"\n=== {s!r} @0x{want:05x} (addr 0x{(base or 0)+want:06x}) refs={len(hits)}")
        for a, k in hits:
            print(f"   code 0x{a:05x} (addr 0x{(base or 0)+a:06x})  [{k}]")


if __name__ == "__main__":
    main()
