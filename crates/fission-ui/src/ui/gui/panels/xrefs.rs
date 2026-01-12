//! Cross-References (Xrefs) window panel.
//!
//! Displays references to and from a selected address.

use crate::analysis::xrefs::{XrefDatabase, XrefType};
use crate::analysis::CallGraph;
use crate::core::config::CONFIG;
use crate::ui::gui::components::widgets::empty_state;
use crate::ui::gui::core::state::AppState;
use crate::ui::gui::core::viewmodels::XrefCallSummary;
use crate::ui::gui::theme::catppuccin;
use eframe::egui;
use fission_loader::loader::FunctionInfo;
use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Action from xrefs window
pub enum XrefAction {
    /// Navigate to an address
    NavigateTo(u64),
    /// No action
    None,
}

fn function_span(func: &FunctionInfo) -> (u64, u64) {
    let fallback_range = CONFIG.analysis.function_address_range as u64;
    let size = if func.size > 0 { func.size } else { fallback_range };
    (func.address, func.address.saturating_add(size))
}

fn find_function_for_addr<'a>(functions: &'a [FunctionInfo], addr: u64) -> Option<&'a FunctionInfo> {
    if functions.is_empty() {
        return None;
    }

    let idx = match functions.binary_search_by_key(&addr, |func| func.address) {
        Ok(index) => index,
        Err(index) => index.checked_sub(1)?,
    };

    let func = &functions[idx];
    let (start, end) = function_span(func);
    if addr >= start && addr < end {
        Some(func)
    } else {
        None
    }
}

fn format_function_label(
    user_names: &HashMap<u64, String>,
    func: Option<&FunctionInfo>,
    addr: u64,
) -> String {
    if let Some(func) = func {
        let icon = if func.is_import {
            "⬇"
        } else if func.is_export {
            "⬆"
        } else {
            "◆"
        };
        let name = user_names
            .get(&func.address)
            .cloned()
            .unwrap_or_else(|| {
                if func.name.is_empty() {
                    format!("sub_{:08X}", func.address)
                } else {
                    func.name.clone()
                }
            });
        format!("{} {} (0x{:08X})", icon, name, func.address)
    } else {
        format!("0x{:08X}", addr)
    }
}

fn build_call_summaries(
    xref_db: &XrefDatabase,
    functions: &[FunctionInfo],
    user_names: &HashMap<u64, String>,
    root_start: u64,
    root_end: u64,
) -> (Vec<XrefCallSummary>, Vec<XrefCallSummary>) {
    let mut callers: HashMap<u64, XrefCallSummary> = HashMap::new();
    let mut callees: HashMap<u64, XrefCallSummary> = HashMap::new();

    for xref in xref_db.iter() {
        if xref.xref_type != XrefType::Call {
            continue;
        }

        if xref.to_addr >= root_start && xref.to_addr < root_end {
            let caller_func = find_function_for_addr(functions, xref.from_addr);
            let caller_addr = caller_func.map(|func| func.address).unwrap_or(xref.from_addr);
            let label = format_function_label(user_names, caller_func, caller_addr);
            let entry = callers
                .entry(caller_addr)
                .or_insert(XrefCallSummary { addr: caller_addr, label, count: 0 });
            entry.count += 1;
        }

        if xref.from_addr >= root_start && xref.from_addr < root_end {
            let callee_func = find_function_for_addr(functions, xref.to_addr);
            let callee_addr = callee_func.map(|func| func.address).unwrap_or(xref.to_addr);
            let label = format_function_label(user_names, callee_func, callee_addr);
            let entry = callees
                .entry(callee_addr)
                .or_insert(XrefCallSummary { addr: callee_addr, label, count: 0 });
            entry.count += 1;
        }
    }

    let mut callers: Vec<XrefCallSummary> = callers.into_values().collect();
    let mut callees: Vec<XrefCallSummary> = callees.into_values().collect();

    callers.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.addr.cmp(&b.addr)));
    callees.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.addr.cmp(&b.addr)));

    (callers, callees)
}

fn build_call_summaries_from_graph(
    call_graph: &CallGraph,
    functions: &[FunctionInfo],
    user_names: &HashMap<u64, String>,
    root_addr: u64,
) -> (Vec<XrefCallSummary>, Vec<XrefCallSummary>) {
    let mut callers: Vec<XrefCallSummary> = call_graph
        .callers_of(root_addr)
        .iter()
        .map(|edge| {
            let func = find_function_for_addr(functions, edge.addr);
            XrefCallSummary {
                addr: edge.addr,
                label: format_function_label(user_names, func, edge.addr),
                count: edge.count,
            }
        })
        .collect();

    let mut callees: Vec<XrefCallSummary> = call_graph
        .callees_of(root_addr)
        .iter()
        .map(|edge| {
            let func = find_function_for_addr(functions, edge.addr);
            XrefCallSummary {
                addr: edge.addr,
                label: format_function_label(user_names, func, edge.addr),
                count: edge.count,
            }
        })
        .collect();

    callers.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.addr.cmp(&b.addr)));
    callees.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.addr.cmp(&b.addr)));

    (callers, callees)
}

fn render_call_summary_list(
    ui: &mut egui::Ui,
    summaries: &[XrefCallSummary],
    action: &mut XrefAction,
) {
    egui::ScrollArea::vertical()
        .max_height(200.0)
        .show(ui, |ui| {
            for summary in summaries {
                ui.horizontal(|ui| {
                    let label = ui.add(
                        egui::Label::new(
                            egui::RichText::new(&summary.label)
                                .monospace()
                                .color(catppuccin::SAPPHIRE),
                        )
                        .sense(egui::Sense::click()),
                    );

                    if label.clicked() {
                        *action = XrefAction::NavigateTo(summary.addr);
                    }
                    if label.hovered() {
                        ui.output_mut(|o| o.cursor_icon = egui::CursorIcon::PointingHand);
                    }

                    ui.label(
                        egui::RichText::new(format!("x{}", summary.count))
                            .small()
                            .color(catppuccin::SUBTEXT0),
                    );
                });
            }
        });
}

fn render_function_view(
    ui: &mut egui::Ui,
    state: &mut AppState,
    addr: u64,
) -> XrefAction {
    let mut action = XrefAction::None;

    let binary = match state.analysis.domain.loaded_binary.as_ref() {
        Some(binary) => binary,
        None => {
            empty_state(ui, "No binary loaded", Some("File → Open to load"));
            return action;
        }
    };

    let range = CONFIG.analysis.function_address_range as u64;
    let mut functions = binary.functions.clone();
    functions.sort_by_key(|func| func.address);
    let selected_func = state.analysis.domain.selected_function.as_ref();
    let root_addr = selected_func
        .map(|func| func.address)
        .or_else(|| find_function_for_addr(&functions, addr).map(|func| func.address))
        .unwrap_or(addr);

    let names_hash = {
        let mut hasher = DefaultHasher::new();
        for (addr, name) in &state.analysis.domain.user_function_names {
            addr.hash(&mut hasher);
            name.hash(&mut hasher);
        }
        hasher.finish()
    };

    let cache_key = (binary.hash.clone(), root_addr, names_hash);
    if state.viewmodels.xrefs.cache_key.as_ref() != Some(&cache_key) {
        let (callers, callees) = if let Some(call_graph) =
            state.analysis.domain.call_graph.as_ref()
        {
            build_call_summaries_from_graph(
                call_graph,
                &functions,
                &state.analysis.domain.user_function_names,
                root_addr,
            )
        } else if let Some(xref_db) = state.analysis.domain.xref_db.as_ref() {
            let (root_start, root_end) = selected_func
                .map(function_span)
                .or_else(|| find_function_for_addr(&functions, addr).map(function_span))
                .unwrap_or((addr, addr.saturating_add(range)));
            build_call_summaries(
                xref_db,
                &functions,
                &state.analysis.domain.user_function_names,
                root_start,
                root_end,
            )
        } else {
            (Vec::new(), Vec::new())
        };
        state.viewmodels.xrefs.callers = callers;
        state.viewmodels.xrefs.callees = callees;
        state.viewmodels.xrefs.cache_key = Some(cache_key);
    }

    let callers = &state.viewmodels.xrefs.callers;
    let callees = &state.viewmodels.xrefs.callees;

    ui.columns(2, |columns| {
        columns[0].vertical(|ui| {
            ui.label(
                egui::RichText::new("CALLERS")
                    .strong()
                    .color(catppuccin::GREEN),
            );
            ui.label(
                egui::RichText::new("(Functions that call this)")
                    .small()
                    .color(catppuccin::SUBTEXT0),
            );
            ui.add_space(4.0);

            if callers.is_empty() {
                ui.label(
                    egui::RichText::new("No callers")
                        .color(catppuccin::OVERLAY0)
                        .small()
                        .italics(),
                );
            } else {
                render_call_summary_list(ui, callers, &mut action);
            }
        });

        columns[1].vertical(|ui| {
            ui.label(
                egui::RichText::new("CALLEES")
                    .strong()
                    .color(catppuccin::PEACH),
            );
            ui.label(
                egui::RichText::new("(Functions called by this)")
                    .small()
                    .color(catppuccin::SUBTEXT0),
            );
            ui.add_space(4.0);

            if callees.is_empty() {
                ui.label(
                    egui::RichText::new("No callees")
                        .color(catppuccin::OVERLAY0)
                        .small()
                        .italics(),
                );
            } else {
                render_call_summary_list(ui, callees, &mut action);
            }
        });
    });

    ui.add_space(8.0);
    ui.separator();
    ui.label(
        egui::RichText::new(format!(
            "Total: {} callers, {} callees",
            callers.len(),
            callees.len()
        ))
        .small()
        .color(catppuccin::SUBTEXT0),
    );

    action
}

/// Render the cross-references window.
/// Returns an action if user clicked on an address.
pub fn render(ctx: &egui::Context, state: &mut AppState) -> XrefAction {
    let mut action = XrefAction::None;

    if !state.ui.show_xrefs_window {
        return action;
    }

    let mut open = state.ui.show_xrefs_window;

    egui::Window::new("🔗 Cross-References")
        .open(&mut open)
        .collapsible(true)
        .resizable(true)
        .default_width(400.0)
        .default_height(300.0)
        .show(ctx, |ui| {
            // Address input
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Address:").color(catppuccin::SUBTEXT0));

                // If we have a selected function, use its address
                if let Some(ref func) = state.analysis.domain.selected_function {
                    let addr = func.address;
                    ui.label(
                        egui::RichText::new(format!("0x{:08X}", addr))
                            .monospace()
                            .color(catppuccin::BLUE),
                    );

                    // Update selected xref addr
                    if state.ui.selected_xref_addr != Some(addr) {
                        state.ui.selected_xref_addr = Some(addr);
                    }
                } else if let Some(addr) = state.ui.selected_xref_addr {
                    ui.label(
                        egui::RichText::new(format!("0x{:08X}", addr))
                            .monospace()
                            .color(catppuccin::BLUE),
                    );
                } else {
                    ui.label(
                        egui::RichText::new("Select a function")
                            .color(catppuccin::OVERLAY0)
                            .italics(),
                    );
                }
            });

            ui.horizontal(|ui| {
                ui.checkbox(&mut state.ui.xrefs_group_by_function, "Group by function");
                ui.label(
                    egui::RichText::new("CALL-only")
                        .small()
                        .color(catppuccin::OVERLAY0),
                );
            });

            ui.separator();

            // Check if we have xref database
            if state.analysis.domain.xref_db.is_none() {
                empty_state(
                    ui,
                    "No cross-references available",
                    Some("Load a binary to analyze references"),
                );
                return;
            }

            if let Some(addr) = state.ui.selected_xref_addr {
                if state.ui.xrefs_group_by_function {
                    action = render_function_view(ui, state, addr);
                } else {
                    let xref_db = state.analysis.domain.xref_db.as_ref().unwrap();
                    // Two columns: REFS TO and REFS FROM
                    ui.columns(2, |columns| {
                        // REFS TO (who calls this address?)
                        columns[0].vertical(|ui| {
                            ui.label(
                                egui::RichText::new("REFS TO")
                                    .strong()
                                    .color(catppuccin::GREEN),
                            );
                            ui.label(
                                egui::RichText::new("(Who calls this?)")
                                    .small()
                                    .color(catppuccin::SUBTEXT0),
                            );
                            ui.add_space(4.0);

                            let refs_to = xref_db.get_refs_to(addr);
                            if refs_to.is_empty() {
                                ui.label(
                                    egui::RichText::new("No references")
                                        .color(catppuccin::OVERLAY0)
                                        .small()
                                        .italics(),
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("refs_to")
                                    .max_height(200.0)
                                    .show(ui, |ui| {
                                        for xref in refs_to {
                                            let type_icon = match xref.xref_type {
                                                XrefType::Call => "📞",
                                                XrefType::Jump => "↪",
                                                XrefType::Data => "📦",
                                            };

                                            let label = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!(
                                                        "{} 0x{:08X}",
                                                        type_icon, xref.from_addr
                                                    ))
                                                    .monospace()
                                                    .color(catppuccin::SAPPHIRE),
                                                )
                                                .sense(egui::Sense::click()),
                                            );

                                            if label.clicked() {
                                                action = XrefAction::NavigateTo(xref.from_addr);
                                            }
                                            if label.hovered() {
                                                ui.output_mut(|o| {
                                                    o.cursor_icon = egui::CursorIcon::PointingHand
                                                });
                                            }
                                        }
                                    });
                            }
                        });

                        // REFS FROM (what does this address call?)
                        columns[1].vertical(|ui| {
                            ui.label(
                                egui::RichText::new("REFS FROM")
                                    .strong()
                                    .color(catppuccin::PEACH),
                            );
                            ui.label(
                                egui::RichText::new("(What does this call?)")
                                    .small()
                                    .color(catppuccin::SUBTEXT0),
                            );
                            ui.add_space(4.0);

                            // For refs from, we need to check all addresses in the function range
                            // For simplicity, just check the function start address
                            let refs_from = xref_db.get_refs_from(addr);
                            if refs_from.is_empty() {
                                ui.label(
                                    egui::RichText::new("No references")
                                        .color(catppuccin::OVERLAY0)
                                        .small()
                                        .italics(),
                                );
                            } else {
                                egui::ScrollArea::vertical()
                                    .id_salt("refs_from")
                                    .max_height(200.0)
                                    .show(ui, |ui| {
                                        for xref in refs_from {
                                            let type_icon = match xref.xref_type {
                                                XrefType::Call => "📞",
                                                XrefType::Jump => "↪",
                                                XrefType::Data => "📦",
                                            };

                                            let label = ui.add(
                                                egui::Label::new(
                                                    egui::RichText::new(format!(
                                                        "{} 0x{:08X}",
                                                        type_icon, xref.to_addr
                                                    ))
                                                    .monospace()
                                                    .color(catppuccin::SAPPHIRE),
                                                )
                                                .sense(egui::Sense::click()),
                                            );

                                            if label.clicked() {
                                                action = XrefAction::NavigateTo(xref.to_addr);
                                            }
                                            if label.hovered() {
                                                ui.output_mut(|o| {
                                                    o.cursor_icon = egui::CursorIcon::PointingHand
                                                });
                                            }
                                        }
                                    });
                            }
                        });
                    });

                    // Summary
                    ui.add_space(8.0);
                    ui.separator();
                    let refs_to_count = xref_db.get_refs_to(addr).len();
                    let refs_from_count = xref_db.get_refs_from(addr).len();
                    ui.label(
                        egui::RichText::new(format!(
                            "Total: {} refs to, {} refs from",
                            refs_to_count, refs_from_count
                        ))
                        .small()
                        .color(catppuccin::SUBTEXT0),
                    );
                }
            } else {
                empty_state(ui, "Select a function to view cross-references", None);
            }
        });

    state.ui.show_xrefs_window = open;
    action
}
