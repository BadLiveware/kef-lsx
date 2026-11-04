import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.lang.*;
import ghidra.program.model.listing.*;
import java.math.BigInteger;

public class Ctx extends GhidraScript {
    public void run() throws Exception {
        Language lang = currentProgram.getLanguage();
        Register base = lang.getContextBaseRegister();
        println("base=" + base);
        Register tmode = null;
        for (Register c : base.getChildRegisters()) {
            if (c.getName().equals("TMode") || c.getName().equals("T")) tmode = c;
        }
        println("tmode=" + tmode);
        if (tmode == null) return;
        AddressSpace sp = currentProgram.getAddressFactory().getDefaultAddressSpace();
        Address s = sp.getAddress(0x400), e = sp.getAddress(0x3bfff);
        currentProgram.getProgramContext().setValue(tmode, s, e, BigInteger.ONE);
        println("readback@0x400=" + currentProgram.getProgramContext().getRegisterValue(tmode, s));
        disassemble(s);
        Instruction i = currentProgram.getListing().getInstructionAt(s);
        println("decoded@0x400 = " + (i == null ? "null" : i.toString()));
    }
}
