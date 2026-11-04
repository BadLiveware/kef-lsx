import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.listing.*;

public class Dump extends GhidraScript {
    public void run() throws Exception {
        AddressSpace sp = currentProgram.getAddressFactory().getDefaultAddressSpace();
        String[] a = getScriptArgs();
        Address s = sp.getAddress(Long.decode(a[0]));
        Address e = sp.getAddress(Long.decode(a[1]));
        Listing lst = currentProgram.getListing();
        disassemble(s);
        InstructionIterator it = lst.getInstructions(s, true);
        int n = 0;
        while (it.hasNext() && n++ < 12) {
            Instruction i = it.next();
            if (i.getAddress().compareTo(e) > 0) break;
            println(i.getAddress() + "  " + i.toString());
        }
    }
}
