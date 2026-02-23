//! RR Command Handlers (Record and Replay)

use crate::ui::cli::handlers::CliState;
use colored::Colorize;
use fission_analysis::debug::TimeTravelDebugger;
use fission_analysis::debug::rr::RRDebugger;
use tracing::warn;

/// Handle 'rr record' command
pub fn cmd_rr_record(binary: &str, args: &[String]) {
    println!("{} {} {:?}", "Recording:".cyan(), binary, args);

    // args needs to be converted to Vec<&str>
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();

    match RRDebugger::record(binary, &args_ref) {
        Ok(trace_path) => {
            println!("{} {}", "Trace created at:".green(), trace_path.display());
            println!(
                "Use 'rr replay {}' to replay this trace",
                trace_path.display()
            );
        }
        Err(e) => {
            eprintln!("{} {}", "Record failed:".red(), e);
        }
    }
}

/// Handle 'rr replay' command
pub fn cmd_rr_replay(_state: &mut CliState, trace_path: Option<String>) {
    let trace_path = if let Some(path) = trace_path {
        std::path::PathBuf::from(path)
    } else {
        match RRDebugger::latest_trace() {
            Some(path) => {
                println!("Replaying latest trace: {}", path.display());
                path
            }
            None => {
                eprintln!("{}", "No trace specified and no latest trace found".red());
                return;
            }
        }
    };

    println!("{} {}", "Replaying trace:".cyan(), trace_path.display());

    let mut rr = RRDebugger::new();
    if let Err(e) = rr.replay(&trace_path) {
        eprintln!("{} {}", "Replay failed:".red(), e);
        return;
    }

    println!(
        "{}",
        "Trace loaded successfully. Entering interactive mode...".green()
    );
    println!(
        "{}",
        "Commands: reverse-step (rs), reverse-continue (rc), seek <N>".dimmed()
    );
    // Sub-REPL for continued RR debugging is not yet implemented.
    // When implemented: enter a loop reading 'rs / rc / seek N / quit' commands,
    // forwarding them to `rr` via the GDB/MI connection and printing results.
    // For now, disconnect cleanly to confirm the trace loaded successfully.
    warn!("rr sub-REPL not yet implemented; disconnecting after trace load");
    rr.stop_recording().ok(); // disconnects / stops replay
}
