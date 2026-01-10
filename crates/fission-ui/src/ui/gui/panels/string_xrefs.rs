//! String cross-reference panel for GUI

use eframe::egui;
use fission_analysis::analysis::string_xrefs::{StringWithXrefs, StringXrefAnalysis};
use fission_analysis::analysis::strings::StringType;

use crate::ui::gui::core::state::{AnalysisState, AppState};

/// Render the string xrefs panel as a window
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    if !state.ui.show_string_xrefs_window {
        return;
    }

    let mut show_window = state.ui.show_string_xrefs_window;

    egui::Window::new("String Cross-References")
        .id(egui::Id::new("string_xrefs_window"))
        .default_width(800.0)
        .default_height(600.0)
        .resizable(true)
        .open(&mut show_window)
        .show(ctx, |ui| {
            // Search controls
            ui.horizontal(|ui| {
                ui.label("Search:");
                ui.add(
                    egui::TextEdit::singleline(&mut state.viewmodels.string_xrefs.search_term)
                        .id(egui::Id::new("string_xref_search_input"))
                        .desired_width(300.0),
                );

                ui.label("Min Length:");
                ui.add(
                    egui::DragValue::new(&mut state.viewmodels.string_xrefs.min_length)
                        .speed(1)
                        .range(1..=100),
                );

                if ui.button("🔍 Analyze").clicked() {
                    perform_analysis(state);
                }

                if ui.button("Clear").clicked() {
                    state.analysis.domain.string_xref_results = None;
                    state.viewmodels.string_xrefs.search_term.clear();
                }
            });

            ui.separator();

            // Show results
            if let Some(ref analysis) = state.analysis.domain.string_xref_results {
                render_results(ui, state, analysis);
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(50.0);
                    ui.label(
                        "Enter a search term and click Analyze to find string cross-references",
                    );
                    ui.add_space(10.0);
                    ui.label("Search modes:");
                    ui.label("  • Partial match: just type the text");
                    ui.label("  • Exact match: use \"quotes\"");
                    ui.label("  • Regex: use /pattern/");
                });
            }
        });

    state.ui.show_string_xrefs_window = show_window;
}

fn perform_analysis(state: &mut AppState) {
    if let Some(ref binary) = state.analysis.domain.loaded_binary {
        let min_len = state.viewmodels.string_xrefs.min_length;
        let analysis =
            fission_analysis::analysis::string_xrefs::analyze_string_xrefs(binary, min_len);
        state.analysis.domain.string_xref_results = Some(analysis);
    }
}

fn render_results(ui: &mut egui::Ui, state: &AppState, analysis: &StringXrefAnalysis) {
    let search_term = &state.viewmodels.string_xrefs.search_term;

    // Get matching strings
    let results = if search_term.is_empty() {
        // Show all referenced strings
        analysis.referenced_strings()
    } else if search_term.starts_with('/') && search_term.ends_with('/') {
        // Regex search
        let pattern = &search_term[1..search_term.len() - 1];
        match analysis.find_by_regex(pattern) {
            Ok(r) => r,
            Err(e) => {
                ui.colored_label(egui::Color32::RED, format!("Invalid regex: {}", e));
                return;
            }
        }
    } else if search_term.starts_with('"') && search_term.ends_with('"') {
        // Exact match
        let exact = &search_term[1..search_term.len() - 1];
        analysis.find_by_content(exact)
    } else {
        // Partial match
        analysis.find_by_partial(search_term)
    };

    // Statistics
    let stats = analysis.stats();
    ui.horizontal(|ui| {
        ui.label(format!("Total Strings: {}", stats.total_strings));
        ui.separator();
        ui.label(format!("Referenced: {}", stats.referenced_strings));
        ui.separator();
        ui.label(format!("Matches: {}", results.len()));
    });
    ui.separator();

    // Results table
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("string_xrefs_grid")
            .striped(true)
            .num_columns(4)
            .show(ui, |ui| {
                // Header
                ui.strong("Address");
                ui.strong("Type");
                ui.strong("String");
                ui.strong("References");
                ui.end_row();

                // Data rows
                for result in results.iter().take(1000) {
                    render_string_row(ui, state, result);
                }
            });
    });
}

fn render_string_row(ui: &mut egui::Ui, state: &AppState, result: &StringWithXrefs) {
    let string = &result.string;

    // Address
    ui.label(format!("0x{:08x}", string.address));

    // Type
    let type_color = match string.string_type {
        StringType::Ascii => egui::Color32::GREEN,
        StringType::Unicode => egui::Color32::LIGHT_BLUE,
    };
    let type_str = match string.string_type {
        StringType::Ascii => "ASCII",
        StringType::Unicode => "UTF16",
    };
    ui.colored_label(type_color, type_str);

    // String content (truncated)
    let display_content = if string.content.len() > 60 {
        format!("{}...", &string.content[..60])
    } else {
        string.content.clone()
    };
    if ui
        .selectable_label(false, &display_content)
        .on_hover_text("Click to copy to clipboard")
        .clicked()
    {
        // Copy to clipboard
        ui.output_mut(|o| o.copied_text = string.content.clone());
    }

    // References count with collapsing header
    let xref_count = result.xrefs.len();
    if xref_count == 0 {
        ui.label("0");
    } else {
        let header_id = egui::Id::new(format!("string_xref_header_{:016x}", string.address));
        egui::CollapsingHeader::new(format!("{} refs", xref_count))
            .id_salt(header_id)
            .show(ui, |ui| {
                for xref in result.xrefs.iter().take(20) {
                    let type_str = match xref.xref_type {
                        fission_analysis::analysis::xrefs::XrefType::Call => "CALL",
                        fission_analysis::analysis::xrefs::XrefType::Jump => "JUMP",
                        fission_analysis::analysis::xrefs::XrefType::Data => "DATA",
                    };

                    // Find function name
                    let caller_name = if let Some(ref binary) = state.analysis.domain.loaded_binary
                    {
                        binary
                            .functions
                            .iter()
                            .find(|f| {
                                xref.from_addr >= f.address && xref.from_addr < f.address + f.size
                            })
                            .map(|f| f.name.as_str())
                            .unwrap_or("unknown")
                    } else {
                        "unknown"
                    };

                    ui.horizontal(|ui| {
                        ui.label(type_str);
                        if ui.link(format!("0x{:08x}", xref.from_addr)).clicked() {
                            // TODO: Navigate to address
                        }
                        ui.label(caller_name);
                    });
                }
                if result.xrefs.len() > 20 {
                    ui.label(format!("... and {} more", result.xrefs.len() - 20));
                }
            });
    }

    ui.end_row();
}
