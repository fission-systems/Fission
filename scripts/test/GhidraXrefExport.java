// Ghidra headless post-script: export instruction-origin references for parity.
//
// Example:
//   analyzeHeadless /tmp/ghidra-project xref-parity -import binary \
//     -scriptPath scripts/test -postScript GhidraXrefExport.java
//
// Output records are stable, tab-separated source/target/type triples prefixed
// with XREF so callers can ignore the surrounding headless log.
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Instruction;
import ghidra.program.model.listing.InstructionIterator;
import ghidra.program.model.symbol.Reference;
import java.util.Set;
import java.util.TreeSet;

public class GhidraXrefExport extends GhidraScript {
    @Override
    public void run() throws Exception {
        Set<String> rows = new TreeSet<>();
        InstructionIterator instructions = currentProgram.getListing().getInstructions(true);
        while (instructions.hasNext() && !monitor.isCancelled()) {
            Instruction instruction = instructions.next();
            for (Reference reference : instruction.getReferencesFrom()) {
                Address target = reference.getToAddress();
                if (!target.isMemoryAddress() || !currentProgram.getMemory().contains(target)) {
                    continue;
                }
                rows.add(String.format(
                    "XREF\t0x%x\t0x%x\t%s",
                    reference.getFromAddress().getOffset(),
                    target.getOffset(),
                    reference.getReferenceType().toString()
                ));
            }
        }
        for (String row : rows) {
            println(row);
        }
        println("XREF_COUNT\t" + rows.size());
    }
}
