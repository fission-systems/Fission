#!/bin/bash
lldb target/release/fission_cli -- inventory function-facts /Users/sjkim1127/Fission/benchmark/binary/x86-64/window/small/binary/c/test_functions.exe --output-jsonl /tmp/out.jsonl --summary-json /tmp/summary.json > lldb_out.txt 2>&1 &
LLDB_PID=$!
sleep 5
kill -INT $LLDB_PID
sleep 1
echo "thread backtrace all" | lldb -p $(pgrep -f "target/release/fission_cli") > bt.txt 2>&1
