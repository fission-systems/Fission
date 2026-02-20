import Editor from "@monaco-editor/react";
import type { editor } from "monaco-editor";
import { useRef } from "react";

interface Props {
    code: string | null;
}

// Catppuccin Mocha theme for Monaco
const catppuccinTheme: editor.IStandaloneThemeData = {
    base: "vs-dark",
    inherit: true,
    rules: [
        { token: "", foreground: "cdd6f4" },
        { token: "comment", foreground: "6c7086", fontStyle: "italic" },
        { token: "keyword", foreground: "cba6f7" },
        { token: "string", foreground: "a6e3a1" },
        { token: "number", foreground: "fab387" },
        { token: "type", foreground: "f9e2af" },
        { token: "function", foreground: "89b4fa" },
        { token: "variable", foreground: "cdd6f4" },
        { token: "operator", foreground: "89dceb" },
        { token: "delimiter", foreground: "9399b2" },
        { token: "constant", foreground: "fab387" },
        { token: "tag", foreground: "f38ba8" },
        { token: "attribute.name", foreground: "f9e2af" },
        { token: "attribute.value", foreground: "a6e3a1" },
    ],
    colors: {
        "editor.background": "#1e1e2e",
        "editor.foreground": "#cdd6f4",
        "editorCursor.foreground": "#f5e0dc",
        "editor.lineHighlightBackground": "#31324455",
        "editorLineNumber.foreground": "#6c7086",
        "editorLineNumber.activeForeground": "#cdd6f4",
        "editor.selectionBackground": "#45475a88",
        "editor.inactiveSelectionBackground": "#45475a44",
        "editorIndentGuide.background": "#31324488",
        "editorIndentGuide.activeBackground": "#45475a",
        "editorWidget.background": "#181825",
        "editorWidget.border": "#313244",
        "minimap.background": "#181825",
    },
};

export default function DecompileView({ code }: Props) {
    const monacoRef = useRef<typeof import("monaco-editor") | null>(null);

    const handleBeforeMount = (monaco: typeof import("monaco-editor")) => {
        monacoRef.current = monaco;
        monaco.editor.defineTheme("catppuccin-mocha", catppuccinTheme);
    };

    if (code === null) {
        return (
            <div className="loading">
                <div className="spinner" />
                Decompiling...
            </div>
        );
    }

    return (
        <Editor
            height="100%"
            language="c"
            theme="catppuccin-mocha"
            value={code}
            beforeMount={handleBeforeMount}
            options={{
                readOnly: true,
                fontSize: 13,
                fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace",
                fontLigatures: true,
                lineNumbers: "on",
                minimap: { enabled: true, scale: 1 },
                scrollBeyondLastLine: false,
                smoothScrolling: true,
                cursorBlinking: "smooth",
                renderLineHighlight: "all",
                padding: { top: 8 },
                bracketPairColorization: { enabled: true },
                guides: { bracketPairs: true, indentation: true },
                glyphMargin: true,
                folding: true,
                wordWrap: "off",
                automaticLayout: true,
            }}
        />
    );
}
