//! Top-level pipeline orchestrator.

use super::concept::GhidraActionConcept;
use super::group::ActionGroup;
use super::pass::PassCtx;
use std::time::Instant;

pub(crate) struct Pipeline {
    pub(crate) name: &'static str,
    pub(crate) groups: Vec<ActionGroup>,
}

impl Pipeline {
    pub(crate) fn new(name: &'static str) -> Self {
        Self {
            name,
            groups: Vec::new(),
        }
    }

    pub(crate) fn group(mut self, group: ActionGroup) -> Self {
        self.groups.push(group);
        self
    }

    pub(crate) fn run(&self, ctx: &mut PassCtx<'_>) {
        let total_start = if ctx.perf { Some(Instant::now()) } else { None };

        if ctx.diag {
            eprintln!("[DIAG] pipeline start: {} fn={}", self.name, ctx.func.name);
        }

        for group in &self.groups {
            group.run(ctx);
        }

        if let Some(start) = total_start {
            eprintln!(
                "[PERF] pipeline total: name={} fn={} elapsed_ms={:.3}",
                self.name,
                ctx.func.name,
                start.elapsed().as_secs_f64() * 1000.0,
            );
        }

        if ctx.diag {
            eprintln!("[DIAG] pipeline done: {} fn={}", self.name, ctx.func.name);
        }
    }

    pub(crate) fn group_names(&self) -> Vec<&'static str> {
        self.groups.iter().map(|group| group.name).collect()
    }
}

pub(crate) fn group(name: &'static str, concept: GhidraActionConcept) -> ActionGroup {
    ActionGroup::new(name, concept)
}
