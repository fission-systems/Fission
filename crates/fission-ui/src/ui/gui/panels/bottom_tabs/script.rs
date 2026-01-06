//! Script tab panel - Python script editor and execution.

use crate::ui::gui::state::AppState;
use crate::ui::gui::theme::catppuccin;
use eframe::egui;

/// Actions that can be triggered from the script panel
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ScriptAction {
    Execute(String),
    Load,
    Save,
    None,
}

/// Render script tab content
pub fn render(ui: &mut egui::Ui, state: &mut AppState) -> ScriptAction {
    // macOS: Python scripting not supported due to PyO3/Cocoa conflicts
    #[cfg(target_os = "macos")]
    {
        let _ = state; // Suppress unused warning
        ui.vertical_centered(|ui| {
            ui.add_space(20.0);
            ui.label(
                egui::RichText::new("⚠ Python Scripting Unavailable")
                    .color(catppuccin::YELLOW)
                    .size(16.0),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Python scripting is not supported on macOS")
                    .color(catppuccin::SUBTEXT0),
            );
            ui.label(
                egui::RichText::new("due to PyO3/Cocoa runtime conflicts.")
                    .color(catppuccin::SUBTEXT0),
            );
            ui.add_space(16.0);
            ui.label(
                egui::RichText::new("Use Windows build for Python scripting.")
                    .color(catppuccin::OVERLAY0)
                    .small(),
            );
        });
        return ScriptAction::None;
    }

    #[cfg(not(target_os = "macos"))]
    {
        let mut action = ScriptAction::None;

        // Toolbar
        ui.horizontal(|ui| {
            let run_text = if state.script.script_running {
                egui::RichText::new("⏳ Running...").color(catppuccin::YELLOW)
            } else {
                egui::RichText::new("▶ Run").color(catppuccin::GREEN)
            };

            if ui
                .add_enabled(!state.script.script_running, egui::Button::new(run_text))
                .clicked()
            {
                let code = state.script.script_code.clone();
                if !code.trim().is_empty() {
                    action = ScriptAction::Execute(code);
                }
            }

            ui.separator();

            // Load/Save buttons
            if ui
                .small_button(egui::RichText::new("📂 Load").color(catppuccin::BLUE))
                .clicked()
            {
                action = ScriptAction::Load;
            }
            if ui
                .small_button(egui::RichText::new("💾 Save").color(catppuccin::TEAL))
                .clicked()
            {
                action = ScriptAction::Save;
            }

            ui.separator();

            if ui
                .small_button(egui::RichText::new("Clear Output").color(catppuccin::RED))
                .clicked()
            {
                state.script.script_output.clear();
            }

            // Show current file path if any
            if let Some(ref path) = state.script.script_path {
                let filename = std::path::Path::new(path)
                    .file_name()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!("📄 {}", filename))
                        .color(catppuccin::SUBTEXT0)
                        .small(),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Keyboard shortcut hint
                ui.label(
                    egui::RichText::new("Ctrl+Enter")
                        .color(catppuccin::SUBTEXT0)
                        .small(),
                );

                #[cfg(feature = "python")]
                ui.label(egui::RichText::new("🐍").color(catppuccin::YELLOW).small());

                #[cfg(not(feature = "python"))]
                ui.label(egui::RichText::new("⚠").color(catppuccin::RED).small());
            });
        });

        ui.separator();

        // Split view: Code editor (top) + Output (bottom)
        let available_height = ui.available_height();
        let editor_height = available_height * 0.55;
        let output_height = available_height * 0.40;

        // Code Editor
        ui.group(|ui| {
            ui.set_min_height(editor_height);
            ui.set_max_height(editor_height);

            ui.label(
                egui::RichText::new("Script Editor")
                    .color(catppuccin::LAVENDER)
                    .strong(),
            );

            egui::ScrollArea::vertical()
                .id_salt("script_editor_scroll")
                .max_height(editor_height - 25.0)
                .show(ui, |ui| {
                    let response = ui.add(
                        egui::TextEdit::multiline(&mut state.script.script_code)
                            .desired_width(ui.available_width())
                            .desired_rows(10)
                            .font(egui::TextStyle::Monospace)
                            .code_editor(),
                    );

                    // Ctrl+Enter to execute
                    if response.has_focus()
                        && ui.input(|i| i.modifiers.ctrl && i.key_pressed(egui::Key::Enter))
                    {
                        let code = state.script.script_code.clone();
                        if !code.trim().is_empty() {
                            action = ScriptAction::Execute(code);
                        }
                    }
                });
        });

        ui.add_space(4.0);

        // Output Area
        ui.group(|ui| {
            ui.set_min_height(output_height);
            ui.set_max_height(output_height);

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("Output")
                        .color(catppuccin::TEAL)
                        .strong(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!("{} lines", state.script.script_output.len()))
                            .color(catppuccin::SUBTEXT0)
                            .small(),
                    );
                });
            });

            egui::ScrollArea::vertical()
                .id_salt("script_output_scroll")
                .max_height(output_height - 25.0)
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &state.script.script_output {
                        let color = get_output_color(line);
                        ui.label(egui::RichText::new(line).color(color).monospace());
                    }

                    if state.script.script_output.is_empty() {
                        ui.label(
                            egui::RichText::new("No output yet. Run a script to see results.")
                                .color(catppuccin::OVERLAY0)
                                .italics(),
                        );
                    }
                });
        });

        action
    }
}

#[allow(dead_code)]
fn get_output_color(line: &str) -> egui::Color32 {
    if line.starts_with("[Python]") {
        catppuccin::YELLOW
    } else if line.starts_with("[Error]") || line.starts_with("Error") {
        catppuccin::RED
    } else if line.starts_with("[✓]") {
        catppuccin::GREEN
    } else if line.starts_with(">>>") {
        catppuccin::MAUVE
    } else {
        catppuccin::TEXT
    }
}
