// Ghidra headless post-script for the source semantic benchmark reference lane.
// This is benchmark-only glue: it exports Ghidra decompiler output as JSON and
// does not make Fission depend on vendor code at build or runtime.

import java.io.FileWriter;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;

import ghidra.app.decompiler.DecompInterface;
import ghidra.app.decompiler.DecompileResults;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.listing.Function;
import ghidra.program.model.listing.FunctionIterator;

public class ExportSourceSemanticDecomp extends GhidraScript {
    private static final class Row {
        String address;
        String name;
        boolean success;
        String code;
        String error;
        double decompileSec;
    }

    @Override
    protected void run() throws Exception {
        String[] args = getScriptArgs();
        if (args.length < 1) {
            throw new IllegalArgumentException("usage: ExportSourceSemanticDecomp.java <output-json> [timeout-sec]");
        }
        String outputPath = args[0];
        int timeoutSec = args.length >= 2 ? Integer.parseInt(args[1]) : 30;
        List<Row> rows = new ArrayList<>();

        DecompInterface decompiler = new DecompInterface();
        try {
            decompiler.openProgram(currentProgram);
            FunctionIterator functions = currentProgram.getFunctionManager().getFunctions(true);
            while (functions.hasNext() && !monitor.isCancelled()) {
                Function function = functions.next();
                Row row = new Row();
                row.address = "0x" + Long.toHexString(function.getEntryPoint().getOffset());
                row.name = function.getName();
                long start = System.nanoTime();
                try {
                    DecompileResults result = decompiler.decompileFunction(function, timeoutSec, monitor);
                    row.decompileSec = (System.nanoTime() - start) / 1_000_000_000.0;
                    if (result.decompileCompleted() && result.getDecompiledFunction() != null) {
                        row.success = true;
                        row.code = result.getDecompiledFunction().getC();
                    }
                    else {
                        row.success = false;
                        row.error = result.getErrorMessage();
                    }
                }
                catch (Exception exc) {
                    row.decompileSec = (System.nanoTime() - start) / 1_000_000_000.0;
                    row.success = false;
                    row.error = exc.toString();
                }
                rows.add(row);
            }
        }
        finally {
            decompiler.dispose();
        }

        writeJson(outputPath, rows);
    }

    private static void writeJson(String outputPath, List<Row> rows) throws IOException {
        try (FileWriter writer = new FileWriter(outputPath)) {
            writer.write("{\n");
            writer.write("  \"schema_version\": 1,\n");
            writer.write("  \"producer\": \"ghidra\",\n");
            writer.write("  \"functions\": [\n");
            for (int i = 0; i < rows.size(); i++) {
                Row row = rows.get(i);
                writer.write("    {\n");
                writer.write("      \"address\": " + json(row.address) + ",\n");
                writer.write("      \"name\": " + json(row.name) + ",\n");
                writer.write("      \"success\": " + row.success + ",\n");
                writer.write("      \"decompile_sec\": " + String.format(java.util.Locale.ROOT, "%.6f", row.decompileSec) + ",\n");
                writer.write("      \"error\": " + json(row.error) + ",\n");
                writer.write("      \"code\": " + json(row.code) + "\n");
                writer.write("    }");
                if (i + 1 < rows.size()) {
                    writer.write(",");
                }
                writer.write("\n");
            }
            writer.write("  ]\n");
            writer.write("}\n");
        }
    }

    private static String json(String value) {
        if (value == null) {
            return "null";
        }
        StringBuilder out = new StringBuilder();
        out.append('"');
        for (int i = 0; i < value.length(); i++) {
            char c = value.charAt(i);
            switch (c) {
                case '"':
                    out.append("\\\"");
                    break;
                case '\\':
                    out.append("\\\\");
                    break;
                case '\b':
                    out.append("\\b");
                    break;
                case '\f':
                    out.append("\\f");
                    break;
                case '\n':
                    out.append("\\n");
                    break;
                case '\r':
                    out.append("\\r");
                    break;
                case '\t':
                    out.append("\\t");
                    break;
                default:
                    if (c < 0x20) {
                        out.append(String.format("\\u%04x", (int) c));
                    }
                    else {
                        out.append(c);
                    }
                    break;
            }
        }
        out.append('"');
        return out.toString();
    }
}
