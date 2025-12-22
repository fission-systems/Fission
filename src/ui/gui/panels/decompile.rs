//! Decompiled code panel - displays C-like decompiled output with syntax highlighting.

use eframe::egui;
use super::super::state::AppState;
use super::super::theme::{catppuccin, code};

/// Render the decompiled code as a fixed right panel.
pub fn render(ctx: &egui::Context, state: &mut AppState) {
    let max_w = ctx.screen_rect().width() * 0.6; // Up to 60% of screen
    egui::SidePanel::right("decompile_panel")
        .resizable(true)
        .default_width(350.0)
        .min_width(150.0)
        .max_width(max_w.max(400.0))
        .show(ctx, |ui| {
            render_inside(ui, state);
        });
}

/// Render decompiled code inside an existing UI.
pub fn render_inside(ui: &mut egui::Ui, state: &mut AppState) {
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
        
        // Copy button on the right
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if !state.decompiled_code.is_empty() {
                if ui.small_button(egui::RichText::new("📋 Copy").color(catppuccin::TEAL)).clicked() {
                    ui.output_mut(|o| o.copied_text = state.decompiled_code.clone());
                }
            }
        });
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
    egui::ScrollArea::both()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Render code with basic syntax highlighting
            render_highlighted_code(ui, &state.decompiled_code);
        });
}

/// Render code with basic C syntax highlighting
fn render_highlighted_code(ui: &mut egui::Ui, code_text: &str) {
    let lines: Vec<&str> = code_text.lines().collect();
    
    // Calculate required width based on longest line
    let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(80);
    let min_width = (max_line_len as f32 * 8.0).max(400.0); // ~8px per char
    
    ui.set_min_width(min_width);
    
    for (line_num, line) in lines.iter().enumerate() {
        ui.horizontal(|ui| {
            // Line number
            ui.label(egui::RichText::new(format!("{:4}", line_num + 1))
                .color(catppuccin::OVERLAY0)
                .monospace());
            
            ui.separator();
            
            // Highlighted code line (no wrapping)
            let highlighted = highlight_c_line(line);
            ui.add(egui::Label::new(highlighted).extend());
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
