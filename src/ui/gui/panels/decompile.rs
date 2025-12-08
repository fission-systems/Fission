//! Decompiled code panel - displays C-like decompiled output with syntax highlighting.

use eframe::egui;
use super::super::state::AppState;
use super::super::theme::{catppuccin, code};

/// Render the decompiled code as a fixed right panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    egui::SidePanel::right("decompile_panel")
        .resizable(true)
        .default_width(400.0)
        .min_width(250.0)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(egui::RichText::new("Decompiled").color(catppuccin::LAVENDER));
                
                if state.decompiling {
                    ui.spinner();
                    ui.label(egui::RichText::new("Processing...")
                        .color(catppuccin::YELLOW).small());
                } else if let Some(ref func) = state.selected_function {
                    ui.separator();
                    ui.label(egui::RichText::new(&func.name)
                        .color(catppuccin::BLUE).small());
                }
            });
            ui.separator();

            if state.decompiled_code.is_empty() && !state.decompiling {
                ui.vertical_centered(|ui| {
                    ui.add_space(60.0);
                    ui.label(egui::RichText::new("No decompilation available")
                        .color(catppuccin::OVERLAY0)
                        .size(14.0));
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Select a function to decompile")
                        .color(catppuccin::OVERLAY0)
                        .small());
                });
                return;
            }

            // Code view with syntax highlighting
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Render code with basic syntax highlighting
                    render_highlighted_code(ui, &state.decompiled_code);
                });
        });
}

/// Render code with basic C syntax highlighting
fn render_highlighted_code(ui: &mut egui::Ui, code_text: &str) {
    let lines: Vec<&str> = code_text.lines().collect();
    
    for (line_num, line) in lines.iter().enumerate() {
        ui.horizontal(|ui| {
            // Line number
            ui.label(egui::RichText::new(format!("{:4}", line_num + 1))
                .color(catppuccin::OVERLAY0)
                .monospace());
            
            ui.separator();
            
            // Highlighted code line
            let highlighted = highlight_c_line(line);
            ui.label(highlighted);
        });
    }
}

/// Apply C syntax highlighting to a single line
fn highlight_c_line(line: &str) -> egui::RichText {
    let trimmed = line.trim();
    
    // Comments
    if trimmed.starts_with("//") || trimmed.starts_with("/*") {
        return egui::RichText::new(line).color(code::COMMENT).monospace();
    }
    
    // Preprocessor directives
    if trimmed.starts_with("#") {
        return egui::RichText::new(line).color(catppuccin::MAUVE).monospace();
    }
    
    // Simple keyword detection
    let keywords = ["if", "else", "while", "for", "return", "break", "continue", 
                   "switch", "case", "default", "do", "goto", "sizeof"];
    let types = ["void", "int", "char", "short", "long", "unsigned", "signed",
                "float", "double", "struct", "union", "enum", "typedef",
                "uint8_t", "uint16_t", "uint32_t", "uint64_t",
                "int8_t", "int16_t", "int32_t", "int64_t", "size_t", "bool"];
    
    // Check if line starts with a type (function definition or declaration)
    for typ in types {
        if trimmed.starts_with(typ) {
            return egui::RichText::new(line).color(code::TYPE).monospace();
        }
    }
    
    // Check for keywords
    for kw in keywords {
        if trimmed.starts_with(kw) && (trimmed.len() == kw.len() || 
            !trimmed.chars().nth(kw.len()).unwrap_or(' ').is_alphanumeric()) {
            return egui::RichText::new(line).color(code::KEYWORD).monospace();
        }
    }
    
    // String literals
    if trimmed.contains('"') {
        return egui::RichText::new(line).color(code::STRING).monospace();
    }
    
    // Function calls (contains parentheses but not control flow)
    if trimmed.contains('(') && !keywords.iter().any(|k| trimmed.starts_with(k)) {
        return egui::RichText::new(line).color(code::FUNCTION).monospace();
    }
    
    // Default
    egui::RichText::new(line).color(catppuccin::TEXT).monospace()
}
