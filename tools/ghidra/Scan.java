// Resolve PC-relative string refs (ldr rX,[pc,#n] ; add rX,pc) and decompile callers.
// args: name=hexoffset ...
import ghidra.app.script.GhidraScript;
import ghidra.app.decompiler.*;
import ghidra.program.model.address.*;
import ghidra.program.model.lang.Register;
import ghidra.program.model.listing.*;
import ghidra.program.model.scalar.Scalar;
import java.util.*;

public class Scan extends GhidraScript {
    static Scalar scalarOf(Instruction ins, int op) {
        for (Object o : ins.getOpObjects(op)) if (o instanceof Scalar) return (Scalar) o;
        return null;
    }
    static boolean hasPc(Instruction ins, int op) {
        for (Object o : ins.getOpObjects(op)) if (o instanceof Register && ((Register) o).getName().equalsIgnoreCase("pc")) return true;
        return false;
    }
    static Register regOf(Instruction ins, int op) {
        for (Object o : ins.getOpObjects(op)) if (o instanceof Register) return (Register) o;
        return null;
    }

    @Override
    public void run() throws Exception {
        AddressSpace sp = currentProgram.getAddressFactory().getDefaultAddressSpace();
        Listing lst = currentProgram.getListing();
        Map<Long,String> want = new LinkedHashMap<>();
        for (String a : getScriptArgs()) {
            String[] p = a.split("=");
            want.put(Long.decode(p[1]), p[0]);
        }
        println("strings=" + want.size() + " insns=" + lst.getNumInstructions());

        List<Instruction> seq = new ArrayList<>();
        InstructionIterator it = lst.getInstructions(true);
        while (it.hasNext()) seq.add(it.next());

        Map<Long,List<Address>> hits = new LinkedHashMap<>();
        for (long k : want.keySet()) hits.put(k, new ArrayList<>());
        int pat = 0;

        for (int i = 0; i < seq.size(); i++) {
            Instruction ins = seq.get(i);
            String m = ins.getMnemonicString().toLowerCase();
            if (!m.startsWith("ldr") || ins.getNumOperands() != 2) continue;
            if (!hasPc(ins, 1)) continue;
            Scalar sc = scalarOf(ins, 1);
            if (sc == null) continue;
            Register rd = regOf(ins, 0);
            if (rd == null) continue;
            long imm = sc.getUnsignedValue();
            pat++;
            for (int j = i + 1; j < Math.min(i + 4, seq.size()); j++) {
                Instruction ins2 = seq.get(j);
                String m2 = ins2.getMnemonicString().toLowerCase();
                if (!m2.startsWith("add") || ins2.getNumOperands() != 2) continue;
                if (!hasPc(ins2, 1)) continue;
                Register rd2 = regOf(ins2, 0);
                if (rd2 == null || !rd2.getName().equalsIgnoreCase(rd.getName())) continue;
                long la = ((ins.getAddress().getOffset() + 4) & ~3L) + imm;
                long litVal;
                try { litVal = currentProgram.getMemory().getLong(sp.getAddress(la)); }
                catch (Exception ex) { break; }
                long pcAdd = (ins2.getAddress().getOffset() + 4) & ~3L;
                long target = litVal + pcAdd;
                if (want.containsKey(target)) hits.get(target).add(ins.getAddress());
                break;
            }
        }
        println("pc-relative ldr sites=" + pat);

        DecompInterface di = new DecompInterface();
        di.toggleCCode(true);
        di.openProgram(currentProgram);
        for (Map.Entry<Long,String> e : want.entrySet()) {
            List<Address> h = hits.get(e.getKey());
            println("\n=== " + e.getValue() + " @0x" + Long.toHexString(e.getKey()) + " refs=" + h.size());
            Set<Function> fns = new LinkedHashSet<>();
            for (Address a : h) {
                Function f = getFunctionContaining(a);
                println("   ref@" + a + " fn=" + (f == null ? "?" : f.getName()));
                if (f != null) fns.add(f);
            }
            for (Function f : fns) {
                DecompileResults res = di.decompileFunction(f, 90, monitor);
                if (res != null && res.decompileCompleted()) {
                    println("   ---- C: " + f.getName() + " @ " + f.getEntryPoint() + " ----");
                    println(res.getDecompiledFunction().getC());
                }
            }
        }
        di.dispose();
    }
}
