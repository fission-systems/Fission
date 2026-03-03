//! Native menu bar — macOS system menu bar, Windows/Linux in-window menu bar.
//!
//! Replaces the React `<MenuBar>` component with OS-native menus via
//! `tauri::menu`. On macOS this renders in the system menu bar at the top of
//! the screen, exactly like VS Code / Xcode / etc.

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Wry};

/// IDs used for menu items.  Keep them as `&str` constants so we can match in
/// `on_menu_event`.
pub mod ids {
    // ── File ──────────────────────────────────────────────────────────
    pub const OPEN_BINARY: &str = "open_binary";
    pub const SAVE_PROJECT: &str = "save_project";
    pub const LOAD_PROJECT: &str = "load_project";
    pub const SAVE_SNAPSHOT: &str = "save_snapshot";
    pub const LOAD_SNAPSHOT: &str = "load_snapshot";
    pub const EXPORT_JSON: &str = "export_json";
    pub const CLEAR_CONSOLE: &str = "clear_console";
    pub const CLEAR_CACHE: &str = "clear_cache";

    // ── Edit ──────────────────────────────────────────────────────────
    pub const GOTO_ADDRESS: &str = "goto_address";
    pub const RENAME_SYMBOL: &str = "rename_symbol";
    pub const ADD_COMMENT: &str = "add_comment";
    pub const DECOMPILER_OPTIONS: &str = "decompiler_options";

    // ── Debug ─────────────────────────────────────────────────────────
    pub const TOGGLE_DYNAMIC: &str = "toggle_dynamic";

    // ── View ──────────────────────────────────────────────────────────
    pub const ASSEMBLY_VIEW: &str = "assembly_view";
    pub const DECOMPILE_VIEW: &str = "decompile_view";
    pub const LISTING_VIEW: &str = "listing_view";
    pub const TOGGLE_SIDEBAR: &str = "toggle_sidebar";
    pub const TOGGLE_BOTTOM: &str = "toggle_bottom";

    // ── Help ──────────────────────────────────────────────────────────
    pub const TOGGLE_DEVTOOLS: &str = "toggle_devtools";
    pub const ABOUT: &str = "about";
}

/// Handles to menu items that need dynamic enable/disable or check toggling.
/// Stored in `AppState` so commands can mutate them.
pub struct MenuHandles {
    // Items disabled when no binary is loaded
    pub save_project: MenuItem<Wry>,
    pub export_json: MenuItem<Wry>,
    pub clear_cache: MenuItem<Wry>,
    pub goto_address: MenuItem<Wry>,
    pub rename_symbol: MenuItem<Wry>,
    pub add_comment: MenuItem<Wry>,
    pub assembly_view: MenuItem<Wry>,
    pub decompile_view: MenuItem<Wry>,
    pub listing_view: MenuItem<Wry>,

    // Check-menu items
    pub toggle_dynamic: CheckMenuItem<Wry>,
    pub toggle_sidebar: CheckMenuItem<Wry>,
    pub toggle_bottom: CheckMenuItem<Wry>,
}

impl MenuHandles {
    /// Enable or disable all binary-dependent items.
    pub fn set_binary_loaded(&self, loaded: bool) {
        let _ = self.save_project.set_enabled(loaded);
        let _ = self.export_json.set_enabled(loaded);
        let _ = self.clear_cache.set_enabled(loaded);
        let _ = self.goto_address.set_enabled(loaded);
        let _ = self.rename_symbol.set_enabled(loaded);
        let _ = self.add_comment.set_enabled(loaded);
        let _ = self.assembly_view.set_enabled(loaded);
        let _ = self.decompile_view.set_enabled(loaded);
        let _ = self.listing_view.set_enabled(loaded);
    }
}

/// Build the full native menu bar.  Returns `(Menu, MenuHandles)`.
pub fn build_menu(app: &AppHandle) -> Result<(Menu<Wry>, MenuHandles), tauri::Error> {
    // ── File ──────────────────────────────────────────────────────────
    let open_binary =
        MenuItem::with_id(app, ids::OPEN_BINARY, "Open Binary…", true, Some("CmdOrCtrl+O"))?;
    let save_project =
        MenuItem::with_id(app, ids::SAVE_PROJECT, "Save Project", false, Some("CmdOrCtrl+S"))?;
    let load_project = MenuItem::with_id(
        app,
        ids::LOAD_PROJECT,
        "Load Project…",
        true,
        Some("CmdOrCtrl+Shift+O"),
    )?;
    let save_snapshot =
        MenuItem::with_id(app, ids::SAVE_SNAPSHOT, "Save Snapshot…", true, None::<&str>)?;
    let load_snapshot =
        MenuItem::with_id(app, ids::LOAD_SNAPSHOT, "Load Snapshot…", true, None::<&str>)?;
    let export_json =
        MenuItem::with_id(app, ids::EXPORT_JSON, "Export Analysis JSON…", false, None::<&str>)?;
    let clear_console =
        MenuItem::with_id(app, ids::CLEAR_CONSOLE, "Clear Console", true, None::<&str>)?;
    let clear_cache =
        MenuItem::with_id(app, ids::CLEAR_CACHE, "Clear Decompile Cache", false, None::<&str>)?;

    let file_menu = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &open_binary,
            &PredefinedMenuItem::separator(app)?,
            &save_project,
            &load_project,
            &PredefinedMenuItem::separator(app)?,
            &save_snapshot,
            &load_snapshot,
            &PredefinedMenuItem::separator(app)?,
            &export_json,
            &PredefinedMenuItem::separator(app)?,
            &clear_console,
            &clear_cache,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some("Exit"))?,
        ],
    )?;

    // ── Edit ──────────────────────────────────────────────────────────
    let goto_address =
        MenuItem::with_id(app, ids::GOTO_ADDRESS, "Go to Address…", false, Some("G"))?;
    let rename_symbol =
        MenuItem::with_id(app, ids::RENAME_SYMBOL, "Rename Symbol", false, Some("N"))?;
    let add_comment =
        MenuItem::with_id(app, ids::ADD_COMMENT, "Add Comment", false, Some(";"))?;
    let decompiler_options = MenuItem::with_id(
        app,
        ids::DECOMPILER_OPTIONS,
        "Decompiler Options…",
        true,
        None::<&str>,
    )?;

    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &goto_address,
            &rename_symbol,
            &add_comment,
            &PredefinedMenuItem::separator(app)?,
            &decompiler_options,
        ],
    )?;

    // ── Debug ─────────────────────────────────────────────────────────
    let toggle_dynamic = CheckMenuItem::with_id(
        app,
        ids::TOGGLE_DYNAMIC,
        "Dynamic Mode",
        true,
        false,
        Some("F5"),
    )?;

    let debug_menu = Submenu::with_items(app, "Debug", true, &[&toggle_dynamic])?;

    // ── View ──────────────────────────────────────────────────────────
    let assembly_view =
        MenuItem::with_id(app, ids::ASSEMBLY_VIEW, "Assembly View", false, None::<&str>)?;
    let decompile_view =
        MenuItem::with_id(app, ids::DECOMPILE_VIEW, "Decompile View", false, None::<&str>)?;
    let listing_view =
        MenuItem::with_id(app, ids::LISTING_VIEW, "Listing View", false, None::<&str>)?;
    let toggle_sidebar = CheckMenuItem::with_id(
        app,
        ids::TOGGLE_SIDEBAR,
        "Side Bar",
        true,
        true,
        Some("CmdOrCtrl+B"),
    )?;
    let toggle_bottom = CheckMenuItem::with_id(
        app,
        ids::TOGGLE_BOTTOM,
        "Bottom Panel",
        true,
        true,
        Some("CmdOrCtrl+J"),
    )?;

    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &assembly_view,
            &decompile_view,
            &listing_view,
            &PredefinedMenuItem::separator(app)?,
            &toggle_sidebar,
            &toggle_bottom,
        ],
    )?;

    // ── Help ──────────────────────────────────────────────────────────
    let toggle_devtools = MenuItem::with_id(
        app,
        ids::TOGGLE_DEVTOOLS,
        "Toggle Developer Tools",
        true,
        Some("CmdOrCtrl+Alt+I"),
    )?;
    let about = MenuItem::with_id(app, ids::ABOUT, "About Fission", true, None::<&str>)?;

    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &[
            &toggle_devtools,
            &PredefinedMenuItem::separator(app)?,
            &about,
        ],
    )?;

    // ── Assemble top-level menu ───────────────────────────────────────
    let menu = Menu::with_items(
        app,
        &[&file_menu, &edit_menu, &debug_menu, &view_menu, &help_menu],
    )?;

    let handles = MenuHandles {
        save_project,
        export_json,
        clear_cache,
        goto_address,
        rename_symbol,
        add_comment,
        assembly_view,
        decompile_view,
        listing_view,
        toggle_dynamic,
        toggle_sidebar,
        toggle_bottom,
    };

    Ok((menu, handles))
}
