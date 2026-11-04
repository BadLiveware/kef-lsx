// Force Thumb mode over given ranges and disassemble linearly.
// args: hexStart-hexEnd ...
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.*;
import java.math.BigInteger;

public class Seed extends GhidraScript {
    @Override
    public void run() throws Exception {
        AddressSpace sp = currentProgram.getAddressFactory().getDefaultAddressSpace();
        Register tmode = currentProgram.getRegister("TB");
        if (tmode == null) tmode = currentProgram.getRegister("TMode");
        Listing lst = currentProgram.getListing();
        println("TMode=" + tmode);
        for (String spec : getScriptArgs()) {
            String[] p = spec.split("-");
            Address s = sp.getAddress(Long.decode(p[0]));
            Address e = sp.getAddress(Long.decode(p[1]));
            if (tmode != null) {
                currentProgram.getProgramContext().setValue(tmode, s, e, BigInteger.ONE);
            }
            Address a = s;
            int made = 0;
            while (a.compareTo(e) <= 0) {
                Instruction ins = lst.getInstructionAt(a);
                if (ins == null) {
                    disassemble(a);
                    ins = lst.getInstructionAt(a);
                }
                if (ins == null) {
                    a = a.add(2);
                } else {
                    made++;
                    a = ins.getMaxAddress().add(1);
                    if ((a.getOffset() & 1) != 0) a = a.add(1);
                }
            }
            println("seeded " + s + " - " + e + " insns=" + made);
        }
        println("TOTAL=" + lst.getNumInstructions());
    }
}
