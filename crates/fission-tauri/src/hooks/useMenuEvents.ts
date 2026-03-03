/**
 * useMenuEvents — listens for native menu bar events emitted from Rust
 * (`app.emit("menu-action", id)`) and dispatches them to the appropriate
 * React callbacks.  This replaces the old `<MenuBar>` component's props.
 */
import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";

export interface UseMenuEventsOptions {
    onOpenFile: () => void;
    onSaveProject: () => void;
    onLoadProject: () => void;
    onSaveSnapshot: () => void;
    onLoadSnapshot: () => void;
    onExportJson: () => void;
    onClearConsole: () => void;
    onClearCache: () => void;
    onGotoAddress: () => void;
    onRenameSymbol: () => void;
    onAddComment: () => void;
    onDecompilerOptions: () => void;
    onToggleDynamic: () => void;
    onAssemblyView: () => void;
    onDecompileView: () => void;
    onListingView: () => void;
    onToggleSidebar: () => void;
    onToggleBottom: () => void;
    onAbout: () => void;
}

export function useMenuEvents(opts: UseMenuEventsOptions) {
    useEffect(() => {
        const unlisten = listen<string>("menu-action", ({ payload }) => {
            switch (payload) {
                case "open_binary":
                    opts.onOpenFile();
                    break;
                case "save_project":
                    opts.onSaveProject();
                    break;
                case "load_project":
                    opts.onLoadProject();
                    break;
                case "save_snapshot":
                    opts.onSaveSnapshot();
                    break;
                case "load_snapshot":
                    opts.onLoadSnapshot();
                    break;
                case "export_json":
                    opts.onExportJson();
                    break;
                case "clear_console":
                    opts.onClearConsole();
                    break;
                case "clear_cache":
                    opts.onClearCache();
                    break;
                case "goto_address":
                    opts.onGotoAddress();
                    break;
                case "rename_symbol":
                    opts.onRenameSymbol();
                    break;
                case "add_comment":
                    opts.onAddComment();
                    break;
                case "decompiler_options":
                    opts.onDecompilerOptions();
                    break;
                case "toggle_dynamic":
                    opts.onToggleDynamic();
                    break;
                case "assembly_view":
                    opts.onAssemblyView();
                    break;
                case "decompile_view":
                    opts.onDecompileView();
                    break;
                case "listing_view":
                    opts.onListingView();
                    break;
                case "toggle_sidebar":
                    opts.onToggleSidebar();
                    break;
                case "toggle_bottom":
                    opts.onToggleBottom();
                    break;
                case "about":
                    opts.onAbout();
                    break;
                default:
                    console.warn("[menu-action] unknown:", payload);
            }
        });

        return () => {
            unlisten.then((f) => f());
        };
    }, [
        opts.onOpenFile,
        opts.onSaveProject,
        opts.onLoadProject,
        opts.onSaveSnapshot,
        opts.onLoadSnapshot,
        opts.onExportJson,
        opts.onClearConsole,
        opts.onClearCache,
        opts.onGotoAddress,
        opts.onRenameSymbol,
        opts.onAddComment,
        opts.onDecompilerOptions,
        opts.onToggleDynamic,
        opts.onAssemblyView,
        opts.onDecompileView,
        opts.onListingView,
        opts.onToggleSidebar,
        opts.onToggleBottom,
        opts.onAbout,
    ]);
}
