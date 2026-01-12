//Converts .fidb files to .fidbf (packed) format for use in Fission
//@category FunctionID
//@author Fission

import java.io.File;
import java.io.IOException;

import db.*;
import ghidra.app.script.GhidraScript;
import ghidra.framework.store.db.PackedDBHandle;
import ghidra.framework.store.db.PackedDatabase;
import ghidra.util.exception.CancelledException;
import ghidra.util.task.TaskMonitor;

public class ConvertFidbToFidbf extends GhidraScript {

    private void copyTable(Table oldTable, PackedDBHandle newHandle) throws IOException, CancelledException {
        String tableName = oldTable.getName();
        Schema schema = oldTable.getSchema();
        int[] indexedColumns = oldTable.getIndexedColumns();

        Table newTable = newHandle.createTable(tableName, schema, indexedColumns);
        monitor.setMessage("Copying table: " + tableName);
        monitor.setMaximum(oldTable.getRecordCount());
        monitor.setProgress(0);

        RecordIterator iterator = oldTable.iterator();
        while (iterator.hasNext()) {
            DBRecord record = iterator.next();
            newTable.putRecord(record);
            monitor.checkCancelled();
            monitor.incrementProgress(1);
        }
    }

    @Override
    protected void run() throws Exception {
        // Get input and output paths from script arguments
        String[] args = getScriptArgs();

        if (args.length < 2) {
            println("Usage: ConvertFidbToFidbf.java <input.fidb> <output.fidbf>");
            println("  input.fidb  - Path to source .fidb file");
            println("  output.fidbf - Path to destination .fidbf file");
            return;
        }

        File inputFile = new File(args[0]);
        File outputFile = new File(args[1]);

        if (!inputFile.exists()) {
            printerr("Input file does not exist: " + inputFile.getAbsolutePath());
            return;
        }

        println("Converting: " + inputFile.getName() + " -> " + outputFile.getName());

        try {
            // Open the source .fidb database
            PackedDatabase pdb = PackedDatabase.getPackedDatabase(inputFile, false, TaskMonitor.DUMMY);
            DBHandle handle = pdb.open(TaskMonitor.DUMMY);

            // Create new packed database handle
            PackedDBHandle newHandle = new PackedDBHandle(pdb.getContentType());

            // Copy all tables
            Table[] tables = handle.getTables();
            println("Found " + tables.length + " tables to copy");

            for (int i = 0; i < tables.length; i++) {
                long transactionID = newHandle.startTransaction();
                copyTable(tables[i], newHandle);
                newHandle.endTransaction(transactionID, true);
            }

            // Save to output file
            newHandle.saveAs(pdb.getContentType(), outputFile.getParentFile(), outputFile.getName(), TaskMonitor.DUMMY);
            newHandle.close();
            handle.close();

            println("SUCCESS: Converted " + inputFile.getName());

        } catch (Exception e) {
            printerr("FAILED: " + inputFile.getName() + " - " + e.getMessage());
            e.printStackTrace();
        }
    }
}
