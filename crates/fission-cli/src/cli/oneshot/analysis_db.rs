//! `fission db` -- read and edit what someone decided about a binary.
//!
//! Everything else this tool prints is derived: run it again and you get it
//! again. What a person or an agent *chose* -- a name for `FUN_00401230`, a
//! note about why a loop matters, a breakpoint that took an hour to find --
//! cannot be re-derived, and this is the only place it goes.
//!
//! Editing loads the binary, because a decision has to be about something
//! that exists: naming an address no function starts at is almost always a
//! typo, and finding that out a week later, from a name that silently never
//! appeared, is worse than being told now.

use anyhow::{Context, Result, bail};
use serde_json::json;

use crate::cli::args::{DbArgs, DbCommand};
use fission_loader::loader::LoadedBinary;
use fission_project::Project;

pub fn run(args: DbArgs) -> Result<()> {
    let binary = LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to read binary at {}", args.binary.display()))?;
    let path = Project::default_path(&args.binary);

    let mut project = match Project::read(&path).map_err(|e| anyhow::anyhow!("{e}"))? {
        Some(existing) => {
            if !existing.matches(&binary) {
                bail!(
                    "{} was written for a different binary; move it aside or point at the \
                     file it describes",
                    path.display()
                );
            }
            existing
        }
        None => Project::for_binary(&binary),
    };

    match args.command {
        DbCommand::Show => return show(&project, &path, args.json),

        DbCommand::Name { addr, name } => {
            require_function(&binary, addr)?;
            let previous = project.set_name(addr, &name);
            report(
                args.json,
                json!({
                    "action": "name",
                    "address": format!("0x{addr:x}"),
                    "name": name,
                    "previous": previous,
                }),
                || match previous.as_deref() {
                    Some(old) => format!("0x{addr:012x}  {old} -> {name}"),
                    None => format!("0x{addr:012x}  {name}"),
                },
            );
        }

        DbCommand::RmName { addr } => {
            let Some(previous) = project.clear_name(addr) else {
                bail!("no name recorded at 0x{addr:x}");
            };
            report(
                args.json,
                json!({
                    "action": "rm-name",
                    "address": format!("0x{addr:x}"),
                    "previous": previous,
                }),
                || format!("0x{addr:012x}  forgot {previous}"),
            );
        }

        DbCommand::Note { addr, text } => {
            project.set_comment(addr, &text);
            report(
                args.json,
                json!({
                    "action": "note",
                    "address": format!("0x{addr:x}"),
                    "text": text,
                }),
                || format!("0x{addr:012x}  {text}"),
            );
        }

        DbCommand::RmNote { addr } => {
            if project.clear_comment(addr).is_none() {
                bail!("no note recorded at 0x{addr:x}");
            }
            report(
                args.json,
                json!({ "action": "rm-note", "address": format!("0x{addr:x}") }),
                || format!("0x{addr:012x}  note removed"),
            );
        }

        DbCommand::Bp { addr } => {
            let added = project.add_breakpoint(addr);
            report(
                args.json,
                json!({
                    "action": "bp",
                    "address": format!("0x{addr:x}"),
                    "added": added,
                }),
                || {
                    if added {
                        format!("0x{addr:012x}  breakpoint")
                    } else {
                        format!("0x{addr:012x}  breakpoint already recorded")
                    }
                },
            );
        }

        DbCommand::RmBp { addr } => {
            if !project.remove_breakpoint(addr) {
                bail!("no breakpoint recorded at 0x{addr:x}");
            }
            report(
                args.json,
                json!({ "action": "rm-bp", "address": format!("0x{addr:x}") }),
                || format!("0x{addr:012x}  breakpoint removed"),
            );
        }
    }

    project
        .write(&path)
        .map_err(|e| anyhow::anyhow!("{e}"))
        .with_context(|| format!("failed to save {}", path.display()))?;
    Ok(())
}

/// A decision has to be about something that exists.
fn require_function(binary: &LoadedBinary, address: u64) -> Result<()> {
    if binary.functions.iter().any(|f| f.address == address) {
        return Ok(());
    }
    let nearest = binary
        .functions
        .iter()
        .filter(|f| f.address <= address)
        .max_by_key(|f| f.address);
    match nearest {
        Some(f) if address < f.address + f.size.max(1) => bail!(
            "0x{address:x} is inside {} but not its entry; name 0x{:x} instead",
            f.name,
            f.address
        ),
        _ => {
            bail!("no function starts at 0x{address:x}; `fission_cli list` shows the ones that do")
        }
    }
}

fn show(project: &Project, path: &std::path::Path, json_out: bool) -> Result<()> {
    if json_out {
        println!("{}", serde_json::to_string_pretty(project)?);
        return Ok(());
    }
    println!("{}", path.display());
    if project.is_empty() {
        println!("  (nothing recorded yet)");
        return Ok(());
    }
    for (address, name) in &project.names {
        println!("  name  0x{address:012x}  {name}");
    }
    for (address, comment) in &project.comments {
        println!("  note  0x{address:012x}  {}", comment.text);
    }
    for address in &project.breakpoints {
        println!("  bp    0x{address:012x}");
    }
    for watch in &project.watchpoints {
        println!(
            "  watch 0x{:012x}  {} bytes  {}{}",
            watch.address,
            watch.size,
            if watch.on_read { "r" } else { "" },
            if watch.on_write { "w" } else { "" },
        );
    }
    Ok(())
}

fn report(json_out: bool, value: serde_json::Value, text: impl FnOnce() -> String) {
    if json_out {
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
        );
    } else {
        println!("{}", text());
    }
}
