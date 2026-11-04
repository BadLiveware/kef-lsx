# Ghidra scripts — not currently working

These are the headless scripts written while trying to get Ghidra to analyse the
LSX firmware. **They do not work**, and they are kept only so the next attempt
does not repeat the dead ends.

The blocker is Thumb mode. Ghidra 12.1.2 exposes `TMode` in `ARM.sinc` as bit 0
of `contextreg`, but it is absent from the compiled `.sla` and is unreachable
from the API by every route tried:

* `lang.getRegister("TMode")` → null
* `currentProgram.getRegister("TMode")` → null
* `getContextRegisters()` → does not include it
* `getContextBaseRegister().getChildRegisters()` → does not include it
* setting the whole 64-bit `contextreg` to 1 → `disassemble()` fails

`ARM:LE:32:v5t` does not decode Thumb either, with or without a base address.

Use `../xref.py` instead — objdump handles `-M force-thumb` correctly. If you
want Ghidra's decompiler, import in the **GUI** (Thumb is a tick-box on the raw
binary import) or build a `.sla` with the context field present.

Files here: `Seed.java` (force mode + linear disassemble), `Scan.java` (string
xref resolution), `Ctx.java` / `Dump.java` (probing helpers).
