//! The whole path, from a command line to a Windows binary being debugged.
//!
//! Every piece below is tested on its own elsewhere. What is not tested
//! elsewhere is that they connect: that `-c "bp 0x140001016"` reaches the
//! emulator's breakpoint set, that the machine survives from one command to
//! the next, and that what comes back is the JSON an agent would read.
//!
//! Static analysis only: the guest instructions are emulated, nothing here
//! runs on the host but the CLI itself.
#![cfg(feature = "debugger")]

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_fission_cli"))
}

fn pe_fixture() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/win_x64_write.exe"
    )
    .to_string()
}

fn elf_fixture() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/x64_concolic_branch_sys.elf"
    )
    .to_string()
}

fn linux_hello_fixture() -> String {
    concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../fission-emulator/testdata/linux_x64_hello_sys.elf"
    )
    .to_string()
}

struct InteractiveCli {
    child: Child,
    input: Option<ChildStdin>,
    responses: Receiver<String>,
}

impl InteractiveCli {
    fn spawn(args: &[&str]) -> Self {
        let mut child = cli()
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start the interactive CLI");
        let input = child.stdin.take().expect("CLI stdin");
        let stdout = child.stdout.take().expect("CLI stdout");
        let (sender, responses) = mpsc::channel();
        thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Self {
            child,
            input: Some(input),
            responses,
        }
    }

    fn send(&mut self, command: &str) {
        let input = self.input.as_mut().expect("session stdin is open");
        writeln!(input, "{command}").expect("write command");
        input.flush().expect("flush command");
    }

    fn close_input(&mut self) {
        self.input.take();
    }

    fn next_response(&self) -> serde_json::Value {
        let line = self
            .responses
            .recv_timeout(Duration::from_secs(5))
            .expect("interactive response arrived before timeout");
        serde_json::from_str(&line).unwrap_or_else(|error| {
            panic!("invalid JSONL response ({error}): {line}");
        })
    }

    fn wait_success(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.child.try_wait().expect("poll CLI status") {
                assert!(status.success(), "interactive CLI exited with {status}");
                return;
            }
            if Instant::now() >= deadline {
                let _ = self.child.kill();
                let _ = self.child.wait();
                panic!("interactive CLI did not exit after the session ended");
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for InteractiveCli {
    fn drop(&mut self) {
        self.input.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

fn run_session(args: &[&str]) -> (bool, serde_json::Value) {
    let output = cli()
        .args(["debug", "--emulator", "session", &pe_fixture(), "--json"])
        .args(args)
        .output()
        .expect("run the CLI");
    let text = String::from_utf8_lossy(&output.stdout);
    // The config banner and the guest's own output share this stream; the
    // report is the JSON document at the end of it.
    let start = text.find('{').unwrap_or_else(|| {
        panic!(
            "no JSON in output.\nstdout: {text}\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        )
    });
    let value = serde_json::from_str(&text[start..])
        .unwrap_or_else(|e| panic!("output is not JSON ({e}): {}", &text[start..]));
    (output.status.success(), value)
}

#[test]
fn a_windows_binary_is_debugged_from_one_command_line() {
    let (ok, report) = run_session(&[
        "-c",
        "bp 0x140001016",
        "-c",
        "continue",
        "-c",
        "regs",
        "-c",
        "continue",
    ]);
    assert!(ok, "the session failed: {report:#}");

    let results = report["results"].as_array().expect("results");
    assert_eq!(results.len(), 4, "{report:#}");

    // The machine survived from the first command to the second: the
    // breakpoint set by one stopped the run started by the next.
    assert_eq!(results[1]["stop"], "breakpoint:0x140001016");
    assert_eq!(results[1]["pc"], "0x140001016");

    // Registers are the machine's, and few enough to read.
    let regs = results[2]["registers"].as_object().expect("registers");
    assert_eq!(regs["pc"], "0x140001016");
    assert!(
        regs.len() < 32,
        "a register dump of {} entries is not a register dump",
        regs.len()
    );
    assert!(regs.contains_key("rsp"));

    // And the program ran to the end, with its own output on the way.
    let events: Vec<&serde_json::Value> = results
        .iter()
        .filter_map(|r| r["events"].as_array())
        .flatten()
        .collect();
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "output"
                && e["message"].as_str().is_some_and(|m| m.contains("hi"))),
        "the program's output never reached the report: {events:#?}"
    );
    assert!(
        events
            .iter()
            .any(|e| e["event"] == "process_exited" && e["exit_code"] == 0),
        "no exit event: {events:#?}"
    );
    assert_eq!(report["final"]["status"], "Terminated");
}

#[test]
fn an_emulated_session_reports_the_backend_it_is_actually_using() {
    let (ok, report) = run_session(&["-c", "capabilities"]);
    assert!(ok, "capability query failed in the session: {report:#}");

    let result = &report["results"][0];
    assert_eq!(result["status"], "ok");
    assert_eq!(result["backend"], "emulator");
    assert_eq!(result["availability"]["status"], "available");
    assert_eq!(result["schema_version"], 1);
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn a_linux_native_session_reports_its_initial_launch_event() {
    let output = cli()
        .args([
            "debug",
            "session",
            "/bin/true",
            "--command",
            "event",
            "--command",
            "modules --json",
            "--command",
            "regs",
            "--command",
            "detach",
            "--json",
        ])
        .output()
        .expect("run native Linux debug session");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "native launch failed.\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let start = stdout.find('{').expect("session JSON report");
    let report: serde_json::Value =
        serde_json::from_str(&stdout[start..]).expect("valid session JSON");
    assert_eq!(report["backend"], "native");
    assert_eq!(report["final"]["status"], "Detached");
    assert_eq!(report["results"][0]["event"]["event"], "process_created");
    assert_eq!(report["results"][0]["event"]["pid"], report["pid"]);
    let modules = report["results"][1]["modules"]
        .as_array()
        .expect("structured module list");
    assert!(!modules.is_empty(), "no file-backed modules: {report:#}");
    let pc = u64::from_str_radix(
        report["results"][2]["registers"]["pc"]
            .as_str()
            .expect("launch-stop PC")
            .trim_start_matches("0x"),
        16,
    )
    .expect("hex PC");
    assert!(
        modules.iter().any(|module| {
            module["address_transform"]["status"] == "resolved"
                && module["mappings"].as_array().is_some_and(|mappings| {
                    mappings.iter().any(|mapping| {
                        let start = u64::from_str_radix(
                            mapping["runtime_start"]
                                .as_str()
                                .unwrap_or("0")
                                .trim_start_matches("0x"),
                            16,
                        )
                        .unwrap_or_default();
                        let end = u64::from_str_radix(
                            mapping["runtime_end"]
                                .as_str()
                                .unwrap_or("0")
                                .trim_start_matches("0x"),
                            16,
                        )
                        .unwrap_or_default();
                        start <= pc && pc < end
                    })
                })
        }),
        "launch PC {pc:#x} did not map to a file-backed ELF transform: {report:#}"
    );
    assert_ne!(
        report["results"][2]["registers"]["pc"], "0x0",
        "the agent could not inspect the launch-stop PC"
    );
    assert_eq!(report["results"][3]["action"], "detach");
}

/// A watchpoint answers "what wrote this", and the answer is two addresses:
/// the memory and the instruction.
#[test]
fn a_watchpoint_reports_the_instruction_that_wrote() {
    let (ok, report) = run_session(&[
        "-c",
        "step",
        "-c",
        "mem-bp 0x7fffffffef00 -s 512 -k write",
        "-c",
        "continue",
    ]);
    assert!(ok, "{report:#}");

    let results = report["results"].as_array().expect("results");
    let hit = results[2]["events"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|e| e["event"] == "watchpoint_hit");
    if let Some(hit) = hit {
        assert_eq!(hit["write"], true);
        assert_ne!(hit["pc"], "0x0", "no instruction named: {hit:#}");
        assert_ne!(
            hit["pc"], hit["address"],
            "the instruction and the memory it wrote are the same address"
        );
    }
}

/// A failed command stops the session and the exit code says so, without a
/// caller having to parse the report to find out.
#[test]
fn a_failed_command_fails_the_session() {
    let (ok, report) = run_session(&["-c", "bp 0x140001016", "-c", "modules"]);
    assert!(!ok, "a refused command reported success: {report:#}");
    let results = report["results"].as_array().expect("results");
    assert_eq!(results.len(), 2, "the session kept going past the failure");
    assert_eq!(results[1]["status"], "error");
    let message = results[1]["error"].as_str().unwrap_or_default();
    assert!(
        message.contains("modules"),
        "the error names the wrong command: {message}"
    );

    let (ok, report) = run_session(&["-c", "modules", "--keep-going", "-c", "regs"]);
    assert!(ok, "--keep-going still failed the session: {report:#}");
    assert_eq!(report["results"].as_array().expect("results").len(), 2);
}

/// Commands can come from a script, one per line, with comments.
#[test]
fn a_script_is_the_same_vocabulary() {
    let dir = std::env::temp_dir().join("fission-session-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("plan.txt");
    std::fs::write(&script, "# two steps\nstep\n\nstep\nregs\n").expect("write script");

    let (ok, report) = run_session(&["--script", script.to_str().expect("path")]);
    assert!(ok, "{report:#}");
    assert_eq!(
        report["results"].as_array().expect("results").len(),
        3,
        "comments and blank lines were not skipped: {report:#}"
    );
    let _ = std::fs::remove_file(&script);
}

/// A command list has no loops and no conditions; a script does. Both drive
/// the same machine, in one session, so the commands can set up what the
/// script then works from.
#[test]
fn a_script_continues_from_where_the_commands_left_the_machine() {
    let dir = std::env::temp_dir().join("fission-session-test");
    std::fs::create_dir_all(&dir).expect("temp dir");
    let script = dir.join("after.rhai");
    std::fs::write(
        &script,
        r#"
        emit(#{ kind: "resumed_at", address: machine.pc() });
        for i in machine.disasm(3) {
            emit(#{ kind: "insn", address: i.address, message: i.text });
        }
        "#,
    )
    .expect("write script");

    let (ok, report) = run_session(&[
        "-c",
        "bp 0x140001016",
        "-c",
        "continue",
        "--rhai",
        script.to_str().expect("path"),
    ]);
    assert!(ok, "{report:#}");

    let script_result = &report["script"];
    assert_eq!(script_result["status"], "ok", "{script_result:#}");
    let findings = script_result["findings"].as_array().expect("findings");

    // The script inherited the machine: it starts where the breakpoint left
    // it, not at the entry point of a freshly launched second copy.
    assert_eq!(findings[0]["kind"], "resumed_at");
    assert_eq!(
        findings[0]["address"], "0x140001016",
        "the script got a different machine than the commands drove"
    );

    // And it can say what is there, which is the other half of stopping.
    assert_eq!(findings[1]["kind"], "insn");
    assert_eq!(findings[1]["address"], "0x140001016");
    assert!(
        findings[1]["message"]
            .as_str()
            .is_some_and(|t| !t.is_empty()),
        "the instruction at the breakpoint disassembled to nothing: {findings:#?}"
    );
    assert_eq!(findings.len(), 4, "{findings:#?}");
}

#[test]
fn interactive_emulator_session_answers_each_command_before_eof() {
    let fixture = elf_fixture();
    let mut session =
        InteractiveCli::spawn(&["debug", "--emulator", "session", &fixture, "--interactive"]);

    let started = session.next_response();
    assert_eq!(started["type"], "session_started");
    assert_eq!(started["backend"], "emulator");
    assert_eq!(started["state"]["status"], "suspended");
    let session_id = started["session_id"]
        .as_str()
        .expect("session id")
        .to_owned();
    let target = started["target"].clone();

    // The error is a response, not a lost session; the next command still
    // reaches the same emulator process.
    session.send("init /tmp/unused-target");
    let failed = session.next_response();
    assert_eq!(failed["type"], "command_result");
    assert_eq!(failed["status"], "error");
    assert_eq!(failed["session_id"], session_id);
    assert_eq!(failed["target"], target);
    assert_eq!(failed["state"]["status"], "suspended");

    session.send("regs");
    let registers = session.next_response();
    assert_eq!(registers["status"], "ok");
    assert_eq!(registers["result"]["registers"]["pc"], "0x400078");
    assert_eq!(registers["session_id"], session_id);
    assert_eq!(registers["target"], target);

    // Choose the next command from the first response while stdin remains
    // open. This is the behavior the former EOF-buffered script could not do.
    let next = match registers["result"]["registers"]["pc"].as_str() {
        Some("0x400078") => "step",
        other => panic!("unexpected initial PC: {other:?}"),
    };
    session.send(next);
    let stepped = session.next_response();
    assert_eq!(stepped["status"], "ok");
    assert_eq!(stepped["result"]["pc"], "0x40007c");
    assert_eq!(stepped["session_id"], session_id);
    assert_eq!(stepped["target"], target);

    session.send("detach");
    let detached = session.next_response();
    assert_eq!(detached["status"], "ok");
    assert_eq!(detached["state"]["status"], "detached");
    let ended = session.next_response();
    assert_eq!(ended["type"], "session_ended");
    assert_eq!(ended["reason"], "detached");
    assert_eq!(ended["state"]["status"], "detached");
    session.wait_success();
}

#[test]
fn interactive_emulator_eof_detaches_and_target_exit_is_reported() {
    let fixture = elf_fixture();
    let mut eof_session =
        InteractiveCli::spawn(&["debug", "--emulator", "session", &fixture, "--interactive"]);
    let started = eof_session.next_response();
    eof_session.close_input();
    let ended = eof_session.next_response();
    assert_eq!(ended["reason"], "input_eof");
    assert_eq!(ended["cleanup"]["status"], "detached");
    assert_eq!(ended["state"]["status"], "detached");
    assert_eq!(ended["session_id"], started["session_id"]);
    eof_session.wait_success();

    let fixture = pe_fixture();
    let mut exit_session =
        InteractiveCli::spawn(&["debug", "--emulator", "session", &fixture, "--interactive"]);
    assert_eq!(exit_session.next_response()["type"], "session_started");
    exit_session.send("continue");
    let continued = exit_session.next_response();
    assert_eq!(continued["status"], "ok", "{continued:#}");
    assert_eq!(continued["state"]["status"], "terminated");
    assert!(
        continued["events"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|event| event["event"] == "output"
                && event["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("hi"))),
        "target output should stay in the structured response: {continued:#}"
    );
    assert!(
        continued["events"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|event| event["event"] == "process_exited")
    );
    let ended = exit_session.next_response();
    assert_eq!(ended["reason"], "target_exited");
    assert_eq!(ended["state"]["status"], "terminated");
    exit_session.wait_success();
}

#[test]
fn interactive_emulator_linux_stdout_is_a_structured_event() {
    let fixture = linux_hello_fixture();
    let mut session =
        InteractiveCli::spawn(&["debug", "--emulator", "session", &fixture, "--interactive"]);

    assert_eq!(session.next_response()["type"], "session_started");
    session.send("continue");
    let continued = session.next_response();
    assert_eq!(continued["status"], "ok", "{continued:#}");
    assert_eq!(continued["state"]["status"], "terminated");
    assert!(
        continued["events"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|event| event["event"] == "output"
                && event["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("hi\n"))),
        "Linux guest stdout should be captured in the structured response: {continued:#}"
    );
    let ended = session.next_response();
    assert_eq!(ended["reason"], "target_exited");
    assert_eq!(ended["state"]["status"], "terminated");
    session.wait_success();
}

#[test]
fn interactive_backend_start_failure_is_structured() {
    let output = cli()
        .args([
            "debug",
            "--emulator",
            "session",
            "--attach",
            "12345",
            "--interactive",
        ])
        .output()
        .expect("run the CLI");
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("invalid start-failure JSON: {error}; {output:?}"));
    assert_eq!(report["type"], "session_start_failed");
    assert_eq!(report["status"], "error");
    assert_eq!(report["backend"], "emulator");
    assert!(
        report["error"]
            .as_str()
            .is_some_and(|message| message.contains("--attach"))
    );
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn interactive_native_attach_replies_and_detaches_on_eof() {
    use std::os::unix::process::CommandExt;

    const PR_SET_PTRACER: i32 = 0x59616d61;
    const PR_SET_PTRACER_ANY: std::ffi::c_ulong = std::ffi::c_ulong::MAX;

    unsafe extern "C" {
        fn prctl(option: std::ffi::c_int, ...) -> std::ffi::c_int;
    }

    let mut target_command = Command::new("/bin/sleep");
    target_command.arg("30");
    // Yama's restricted ptrace policy allows the test CLI to attach to this
    // child without changing the host policy.
    unsafe {
        target_command.pre_exec(|| {
            let result = unsafe {
                prctl(
                    PR_SET_PTRACER,
                    PR_SET_PTRACER_ANY,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                    0 as std::ffi::c_ulong,
                )
            };
            if result == -1 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut target = target_command
        .spawn()
        .expect("start attachable Linux target");
    let pid = target.id();
    let pid_arg = pid.to_string();
    let mut session =
        InteractiveCli::spawn(&["debug", "session", "--attach", &pid_arg, "--interactive"]);

    let started = session.next_response();
    assert_eq!(started["type"], "session_started");
    assert_eq!(started["backend"], "native");
    assert_eq!(started["target"]["pid"], pid);
    let session_id = started["session_id"].clone();
    session.send("regs");
    let registers = session.next_response();
    assert_eq!(registers["status"], "ok", "{registers:#}");
    assert_eq!(registers["target"]["pid"], pid);
    assert_eq!(registers["session_id"], session_id);

    session.close_input();
    let ended = session.next_response();
    assert_eq!(ended["reason"], "input_eof");
    assert_eq!(ended["cleanup"]["status"], "detached");
    assert_eq!(ended["state"]["status"], "detached");
    session.wait_success();

    assert!(target.try_wait().expect("poll attached target").is_none());
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .expect("attached target is still present");
    let process_state = stat
        .rsplit_once(')')
        .and_then(|(_, tail)| tail.split_whitespace().next())
        .expect("Linux process state");
    assert_ne!(process_state, "t", "target remained ptrace-stopped");
    assert_ne!(process_state, "T", "target remained job-control stopped");
    target.kill().expect("clean up attached test target");
    target.wait().expect("reap attached test target");
}

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[test]
fn interactive_native_launch_reports_exit_events() {
    let mut session = InteractiveCli::spawn(&["debug", "session", "/bin/true", "--interactive"]);
    let started = session.next_response();
    assert_eq!(started["state"]["status"], "suspended");

    session.send("regs");
    let registers = session.next_response();
    assert_eq!(registers["status"], "ok");
    assert_eq!(registers["target"]["pid"], started["target"]["pid"]);

    session.send("continue");
    let continued = session.next_response();
    assert_eq!(continued["status"], "ok", "{continued:#}");
    if continued["state"]["status"] == "running" {
        session.send("event --timeout-ms 2000");
        let event = session.next_response();
        assert_eq!(event["status"], "ok", "{event:#}");
        assert_eq!(event["result"]["event"]["event"], "process_exited");
    } else {
        assert_eq!(continued["state"]["status"], "terminated", "{continued:#}");
        assert!(
            continued["events"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|event| event["event"] == "process_exited")
        );
    }
    let ended = session.next_response();
    assert_eq!(ended["reason"], "target_exited");
    assert_eq!(ended["state"]["status"], "terminated");
    session.wait_success();
}
