//! Automation report: snapshots, deltas, Markdown, baselines.

mod baseline_io;
mod delta;
mod insights;
mod quality;
mod render;
mod snapshot;
mod summary_build;

pub use baseline_io::{load_baseline, load_baseline_candidates, update_latest};
pub use delta::{SummaryDelta, compute_delta};
pub use insights::{AutomationDecisionInsights, build_decision_insights};
pub use quality::build_quality_measurement;
pub use render::{print_terminal_summary, render_markdown};
pub use snapshot::AutomationSummary;
pub use summary_build::{build_summary, enrich_summary_with_provenance};
