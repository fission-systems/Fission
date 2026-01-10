//! CFG (Control Flow Graph) Analysis panel.
//!
//! Displays CFG analysis including blocks, edges, loops, and metrics.

use eframe::egui;

use crate::ui::gui::components::widgets::empty_state_with_spacing;
use crate::ui::gui::core::state::AppState;
use crate::ui::gui::theme::catppuccin;

/// Actions that can be triggered from the CFG panel
pub enum CfgAction {
    /// No action needed
    None,
    /// Request CFG analysis for address
    Analyze(u64),
}

/// Render the CFG analysis panel.
pub fn render(ui: &mut egui::Ui, state: &mut AppState) -> CfgAction {
    // Extract function info to avoid borrow issues
    let func_info = state
        .analysis
        .selected_function
        .as_ref()
        .map(|f| (f.name.clone(), f.address));
    let has_cfg = state.analysis.cfg_analysis.is_some();
    let mut request_analysis = false;
    let mut request_export = false;

    ui.horizontal(|ui| {
        ui.heading(egui::RichText::new("CFG Analysis").color(catppuccin::LAVENDER));

        // Add analysis button
        if let Some((name, _addr)) = &func_info {
            ui.separator();
            ui.label(egui::RichText::new(name).color(catppuccin::BLUE).small());

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button(egui::RichText::new("Analyze CFG").color(catppuccin::TEAL))
                    .clicked()
                {
                    request_analysis = true;
                }

                if has_cfg {
                    if ui
                        .small_button(egui::RichText::new("Export DOT").color(catppuccin::GREEN))
                        .clicked()
                    {
                        request_export = true;
                    }
                }
            });
        }
    });

    // Handle actions after borrowing is released
    if request_analysis {
        if let Some((name, addr)) = func_info {
            state.log(format!(
                "[*] Requesting CFG analysis for {} @ 0x{:x}",
                name, addr
            ));
            if request_export {
                export_cfg_dot(state);
            }
            return CfgAction::Analyze(addr);
        }
    }
    if request_export {
        export_cfg_dot(state);
    }

    ui.separator();

    // Check if we have CFG analysis data
    if let Some(ref cfg_result) = state.analysis.cfg_analysis {
        render_cfg_content(ui, cfg_result);
    } else {
        empty_state_with_spacing(
            ui,
            "No CFG analysis available",
            Some("Select a function and click 'Analyze CFG'"),
            40.0,
        );
    }

    CfgAction::None
}

/// Render CFG analysis content
fn render_cfg_content(ui: &mut egui::Ui, cfg: &CfgAnalysisResult) {
    // Metrics section
    egui::CollapsingHeader::new(egui::RichText::new("Metrics").color(catppuccin::MAUVE))
        .default_open(true)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Blocks:").color(catppuccin::TEXT));
                ui.label(
                    egui::RichText::new(format!("{}", cfg.block_count)).color(catppuccin::BLUE),
                );
                ui.separator();
                ui.label(egui::RichText::new("Edges:").color(catppuccin::TEXT));
                ui.label(
                    egui::RichText::new(format!("{}", cfg.edge_count)).color(catppuccin::BLUE),
                );
                ui.separator();
                ui.label(egui::RichText::new("Cyclomatic:").color(catppuccin::TEXT));
                ui.label(
                    egui::RichText::new(format!("{}", cfg.cyclomatic_complexity))
                        .color(catppuccin::YELLOW),
                );
                ui.separator();
                ui.label(egui::RichText::new("Max Depth:").color(catppuccin::TEXT));
                ui.label(
                    egui::RichText::new(format!("{}", cfg.max_nesting_depth))
                        .color(catppuccin::PEACH),
                );
            });
        });

    ui.add_space(4.0);

    // Loops section
    if !cfg.loops.is_empty() {
        egui::CollapsingHeader::new(
            egui::RichText::new(format!("Loops ({})", cfg.loops.len())).color(catppuccin::MAUVE),
        )
        .default_open(true)
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .show(ui, |ui| {
                    for (i, loop_info) in cfg.loops.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("Loop {}:", i))
                                    .color(catppuccin::GREEN),
                            );
                            ui.label(
                                egui::RichText::new(format!("Header=BB{}", loop_info.header))
                                    .color(catppuccin::BLUE),
                            );
                            ui.label(
                                egui::RichText::new(format!("Kind={}", loop_info.kind))
                                    .color(catppuccin::PEACH),
                            );
                            ui.label(
                                egui::RichText::new(format!("Body={:?}", loop_info.body))
                                    .color(catppuccin::OVERLAY0)
                                    .small(),
                            );
                        });
                    }
                });
        });
    }

    ui.add_space(4.0);

    // Blocks section
    egui::CollapsingHeader::new(
        egui::RichText::new(format!("Basic Blocks ({})", cfg.block_count)).color(catppuccin::MAUVE),
    )
    .default_open(false)
    .show(ui, |ui| {
        egui::ScrollArea::vertical()
            .max_height(200.0)
            .show(ui, |ui| {
                for block in &cfg.blocks {
                    ui.horizontal(|ui| {
                        let marker = if block.is_entry {
                            egui::RichText::new("[ENTRY]").color(catppuccin::GREEN)
                        } else if block.is_exit {
                            egui::RichText::new("[EXIT]").color(catppuccin::RED)
                        } else {
                            egui::RichText::new("").color(catppuccin::TEXT)
                        };

                        ui.label(
                            egui::RichText::new(format!("BB{}", block.index))
                                .color(catppuccin::BLUE),
                        );
                        ui.label(
                            egui::RichText::new(format!("@ {}", block.address))
                                .color(catppuccin::TEAL)
                                .small(),
                        );
                        ui.label(marker);

                        if !block.successors.is_empty() {
                            ui.label(
                                egui::RichText::new(format!("-> {:?}", block.successors))
                                    .color(catppuccin::OVERLAY0)
                                    .small(),
                            );
                        }
                    });
                }
            });
    });
}

/// Export CFG to DOT file
fn export_cfg_dot(state: &mut AppState) {
    if let Some(ref cfg_result) = state.analysis.cfg_analysis {
        // Generate DOT content
        let dot_content = &cfg_result.dot_content;

        // Try to write to file
        let filename = format!(
            "cfg_{:x}.dot",
            state
                .analysis
                .selected_function
                .as_ref()
                .map(|f| f.address)
                .unwrap_or(0)
        );

        match std::fs::write(&filename, dot_content) {
            Ok(_) => {
                state.log(format!("[✓] CFG exported to: {}", filename));
            }
            Err(e) => {
                state.log(format!("[!] Failed to export CFG: {}", e));
            }
        }
    }
}

/// CFG analysis result for display
#[derive(Debug, Clone, Default)]
pub struct CfgAnalysisResult {
    pub block_count: usize,
    pub edge_count: usize,
    pub cyclomatic_complexity: usize,
    pub max_nesting_depth: usize,
    pub loops: Vec<LoopDisplayInfo>,
    pub blocks: Vec<BlockDisplayInfo>,
    pub dot_content: String,
}

/// Loop info for display
#[derive(Debug, Clone)]
pub struct LoopDisplayInfo {
    pub header: usize,
    pub kind: String,
    pub body: Vec<usize>,
}

/// Block info for display
#[derive(Debug, Clone)]
pub struct BlockDisplayInfo {
    pub index: usize,
    pub address: String,
    pub is_entry: bool,
    pub is_exit: bool,
    pub successors: Vec<usize>,
}

impl CfgAnalysisResult {
    /// Create from CfgAnalysis
    #[cfg(feature = "native_decomp")]
    pub fn from_analysis(analysis: &fission_analysis::analysis::cfg::CfgAnalysis) -> Self {
        use fission_analysis::analysis::cfg::{CfgVisualizer, DotOptions};

        let loops: Vec<LoopDisplayInfo> = analysis
            .loops
            .iter()
            .map(|l| LoopDisplayInfo {
                header: l.header,
                kind: format!("{:?}", l.kind),
                body: l.body.iter().copied().collect(),
            })
            .collect();

        let blocks: Vec<BlockDisplayInfo> = analysis
            .cfg
            .blocks
            .iter()
            .map(|b| BlockDisplayInfo {
                index: b.index,
                address: format!("0x{:x}", b.start_address),
                is_entry: b.is_entry,
                is_exit: b.is_exit,
                successors: b.successors.iter().map(|e| e.target).collect(),
            })
            .collect();

        let dot_options = DotOptions {
            show_instructions: true,
            show_addresses: true,
            highlight_loops: true,
            ..Default::default()
        };
        let dot_content = CfgVisualizer::to_dot(&analysis.cfg, &analysis.loops, &dot_options);

        CfgAnalysisResult {
            block_count: analysis.cfg.block_count(),
            edge_count: analysis.cfg.edge_count(),
            cyclomatic_complexity: analysis.metrics.cyclomatic_complexity,
            max_nesting_depth: analysis.metrics.max_nesting_depth,
            loops,
            blocks,
            dot_content,
        }
    }
}
