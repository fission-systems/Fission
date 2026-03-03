import { useEffect } from "react";
import type { EditorTab, BinaryInfo } from "../types";

interface UseKeyboardShortcutsOptions {
    binaryInfo: BinaryInfo | null;
    tabs: EditorTab[];
    activeTabId: string | null;
    /** Open the binary file picker */
    onOpenFile: () => void;
    /** Toggle the active tab's bookmark */
    onToggleBookmark: () => void;
    /** Trigger undo */
    onUndo: () => void;
    /** Trigger redo */
    onRedo: () => void;
    /** Navigate back in history */
    onGoBack: () => void;
    /** Navigate forward in history */
    onGoForward: () => void;
    /** Open the Goto Address dialog */
    onOpenGoto: () => void;
    /** Open the Rename dialog for a given address/name */
    onOpenRename: (address: string, name: string) => void;
    /** Open the Comment dialog for a given address/comment */
    onOpenComment: (address: string, comment: string) => void;
    /** Toggle the bottom panel visibility */
    onToggleBottomPanel: () => void;
    /** Toggle developer tools */
    onToggleDevTools: () => void;
}

export function useKeyboardShortcuts({
    binaryInfo,
    tabs,
    activeTabId,
    onOpenFile,
    onToggleBookmark,
    onUndo,
    onRedo,
    onGoBack,
    onGoForward,
    onOpenGoto,
    onOpenRename,
    onOpenComment,
    onToggleBottomPanel,
    onToggleDevTools,
}: UseKeyboardShortcutsOptions) {
    useEffect(() => {
        const handleKeyDown = (e: KeyboardEvent) => {
            // Ignore when typing in an input
            const tag = (e.target as HTMLElement).tagName;
            if (tag === "INPUT" || tag === "TEXTAREA") return;

            // Ctrl+O: Open file
            if (e.ctrlKey && e.key === "o") {
                e.preventDefault();
                onOpenFile();
                return;
            }

            // G: Go to address
            if (e.key === "g" && !e.ctrlKey && !e.altKey && binaryInfo) {
                e.preventDefault();
                onOpenGoto();
                return;
            }

            // Ctrl+Z: Undo
            if (e.ctrlKey && e.key === "z" && !e.shiftKey) {
                e.preventDefault();
                onUndo();
                return;
            }

            // Ctrl+Y or Ctrl+Shift+Z: Redo
            if ((e.ctrlKey && e.key === "y") || (e.ctrlKey && e.shiftKey && e.key === "z")) {
                e.preventDefault();
                onRedo();
                return;
            }

            // N: Rename
            if (e.key === "n" && !e.ctrlKey && !e.altKey) {
                e.preventDefault();
                const tab = tabs.find((t) => t.id === activeTabId);
                if (tab) onOpenRename(tab.address, tab.functionName);
                return;
            }

            // ;: Comment
            if (e.key === ";" && !e.ctrlKey && !e.altKey) {
                e.preventDefault();
                const tab = tabs.find((t) => t.id === activeTabId);
                if (tab) onOpenComment(tab.address, "");
                return;
            }

            // F2: Bookmark
            if (e.key === "F2") {
                e.preventDefault();
                onToggleBookmark();
                return;
            }

            // Alt+Left or Cmd+Left: Back
            if ((e.altKey || e.metaKey) && e.key === "ArrowLeft") {
                e.preventDefault();
                onGoBack();
                return;
            }

            // Alt+Right or Cmd+Right: Forward
            if ((e.altKey || e.metaKey) && e.key === "ArrowRight") {
                e.preventDefault();
                onGoForward();
                return;
            }

            // Ctrl+J: Toggle bottom panel
            if (e.ctrlKey && e.key === "j") {
                e.preventDefault();
                onToggleBottomPanel();
                return;
            }

            // Cmd+Option+I (Mac) / Ctrl+Shift+I (Win/Linux): Toggle DevTools
            if ((e.metaKey && e.altKey && e.key === "i") || (e.ctrlKey && e.shiftKey && e.key === "I")) {
                e.preventDefault();
                onToggleDevTools();
                return;
            }
        };

        window.addEventListener("keydown", handleKeyDown);
        return () => window.removeEventListener("keydown", handleKeyDown);
    }, [
        binaryInfo,
        tabs,
        activeTabId,
        onOpenFile,
        onToggleBookmark,
        onUndo,
        onRedo,
        onGoBack,
        onGoForward,
        onOpenGoto,
        onOpenRename,
        onOpenComment,
        onToggleBottomPanel,
        onToggleDevTools,
    ]);
}
