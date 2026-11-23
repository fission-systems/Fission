//! `fission_cli quotient`: the minimal observable state machine of one step
//! function. The engine is `fission-quotient` (experimental, RFC #1093); this
//! command only builds its boundary from the flags and prints its report.

use crate::cli::args::QuotientArgs;
use anyhow::{Context, Result};
use fission_quotient::report::CertifiedQuotient;
use fission_quotient::{
    ContextRegion, InputPort, QuotientBoundary, QuotientBudget, QuotientEngine, QuotientStatus,
    Wbcq,
};

fn boundary(args: &QuotientArgs, entry: u64) -> QuotientBoundary {
    let inputs = if args.inputs.is_empty() {
        vec![InputPort {
            name: "input".to_string(),
            arg_index: 1,
            bits: 32,
        }]
    } else {
        args.inputs
            .iter()
            .map(|port| InputPort {
                name: port.name.clone(),
                arg_index: port.arg_index,
                bits: port.bits,
            })
            .collect()
    };
    let initial = match &args.context_init {
        Some(bytes) => bytes.0.clone(),
        None => vec![0; args.context_bytes],
    };
    QuotientBoundary {
        entry,
        inputs,
        context: ContextRegion {
            arg_index: args.context_arg,
            initial,
        },
        output_bits: args.output_bits,
    }
}

fn budget(args: &QuotientArgs) -> QuotientBudget {
    QuotientBudget {
        max_states: args.max_states,
        horizon: args.horizon,
        conflict_budget: Some(args.conflict_budget),
        ..QuotientBudget::default()
    }
}

pub(crate) fn run(args: &QuotientArgs) -> Result<()> {
    let binary = fission_loader::loader::LoadedBinary::from_file(&args.binary)
        .with_context(|| format!("failed to read binary at {}", args.binary.display()))?;
    let entry = match (args.addr, args.name.as_deref()) {
        (Some(address), _) => address,
        (None, Some(name)) => {
            let mut binary = binary.clone();
            fission_static::analysis::discover_functions_with_runtime(&mut binary);
            super::function_select::select_function_address_by_name(&binary, name)
                .map_err(crate::cli::contract::FunctionSelectorError)?
        }
        (None, None) => anyhow::bail!("give --addr or --name"),
    };
    if args.context_init.is_none() && args.context_bytes == 0 {
        anyhow::bail!("--context-bytes must be at least 1");
    }

    let (mut engine, foundation) =
        Wbcq::for_binary(&args.binary, boundary(args, entry), args.max_inst)
            .context("failed to set up the quotient engine for this function")?;
    let report = engine.synthesize(&foundation, budget(args))?;

    if args.json {
        println!("{}", report.to_json()?);
    } else {
        print_text(&report);
    }
    Ok(())
}

fn symbols(word: &[Vec<u64>]) -> String {
    let items = word
        .iter()
        .map(|symbol| {
            let values = symbol
                .iter()
                .map(|value| format!("0x{value:x}"))
                .collect::<Vec<_>>();
            if values.len() == 1 {
                values[0].clone()
            } else {
                format!("({})", values.join(", "))
            }
        })
        .collect::<Vec<_>>();
    format!("[{}]", items.join(", "))
}

fn print_text(report: &CertifiedQuotient) {
    let status = match &report.status {
        QuotientStatus::ProvedUnderModel { horizon, model_id } => {
            format!("proved under model {model_id} for words of up to {horizon} steps")
        }
        QuotientStatus::EmpiricallyConsistent {
            horizon,
            probes_run,
            solver_unknowns,
        } => format!(
            "empirically consistent ({probes_run} probes of {horizon} steps; the solver could \
             not decide {solver_unknowns} queries) -- evidence, not proof"
        ),
        QuotientStatus::PartialFrontier {
            max_states_hit,
            unmodeled_edges,
        } => format!(
            "partial: at least {max_states_hit} states ({unmodeled_edges} edges unmodeled); \
             raise --max-states"
        ),
        QuotientStatus::Refuted { counterexample_hex } => format!(
            "refuted: the input word [{counterexample_hex}] changes a data field the control \
             states cannot express"
        ),
        QuotientStatus::UnsupportedSemantics { reason } => format!("unsupported: {reason}"),
    };
    println!("quotient 0x{:x}: {status}", report.boundary.entry);
    let metrics = &report.metrics;
    println!(
        "  P-code: {} blocks, {} edges, {} feasible paths per step, {} dead branch side(s) pruned",
        metrics.pcode_blocks,
        metrics.pcode_edges,
        metrics.feasible_paths,
        metrics.pruned_branches.len()
    );
    if !report.slots.is_empty() {
        let slots = report
            .slots
            .iter()
            .map(|slot| {
                let role = match slot.role {
                    fission_quotient::report::SlotRole::Control => "control",
                    fission_quotient::report::SlotRole::Data => "data",
                };
                format!("{} {role}", slot.name)
            })
            .collect::<Vec<_>>();
        println!("  context: {}", slots.join(", "));
    }
    if report.states.is_empty() {
        return;
    }
    println!(
        "  {} states, {} transitions",
        metrics.quotient_states, metrics.quotient_transitions
    );
    for state in &report.states {
        let valuation = state
            .control_valuation
            .iter()
            .map(|value| format!("{}=0x{:x}", value.slot, value.value))
            .collect::<Vec<_>>();
        println!(
            "  state {}: reached by {}{}",
            state.id,
            symbols(&state.access_word),
            if valuation.is_empty() {
                String::new()
            } else {
                format!(", {}", valuation.join(" "))
            }
        );
    }
    for transition in &report.transitions {
        let updates = transition
            .updates
            .iter()
            .map(|update| format!("{} := {}", update.slot, update.value))
            .collect::<Vec<_>>();
        println!(
            "  {} -> {} if {} / returns {}{}",
            transition.from,
            transition.to,
            transition.guard,
            transition.output,
            if updates.is_empty() {
                String::new()
            } else {
                format!("; {}", updates.join(", "))
            }
        );
    }
    for witness in &report.apartness_witnesses {
        println!(
            "  states {} and {} differ on {}{}",
            witness.state_a,
            witness.state_b,
            symbols(&witness.input_sequence),
            if witness.verified_by_emulator {
                ""
            } else {
                " (did not reproduce)"
            }
        );
    }
}
