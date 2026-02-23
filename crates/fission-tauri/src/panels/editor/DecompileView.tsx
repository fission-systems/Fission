import { useState, useMemo, useCallback, useRef } from "react";

interface DecompileViewProps {
    code: string | null;
    functionName?: string;
    onSymbolClick?: (symbol: string) => void;
    onRename?: (symbol: string) => void;
}

// Simple C-like tokenizer for syntax highlighting
interface Token {
    type: "keyword" | "type" | "number" | "string" | "comment" | "symbol" | "operator" | "paren" | "text";
    value: string;
}

const KEYWORDS = new Set([
    "if", "else", "while", "for", "do", "switch", "case", "default", "return",
    "break", "continue", "goto", "struct", "union", "enum", "typedef",
    "sizeof", "typeof", "const", "static", "volatile", "extern", "register",
    "inline", "void", "NULL", "true", "false",
]);

const TYPES = new Set([
    "int", "long", "short", "char", "float", "double", "unsigned", "signed",
    "uint8_t", "uint16_t", "uint32_t", "uint64_t",
    "int8_t", "int16_t", "int32_t", "int64_t",
    "size_t", "ssize_t", "uintptr_t", "intptr_t",
    "bool", "BOOL", "BYTE", "WORD", "DWORD", "QWORD",
    "HANDLE", "HMODULE", "FARPROC", "LPSTR", "LPCSTR", "LPWSTR", "LPCWSTR",
    "PVOID", "LPVOID", "HRESULT", "NTSTATUS", "ULONG", "UCHAR",
]);

function tokenize(line: string): Token[] {
    const tokens: Token[] = [];
    let i = 0;

    while (i < line.length) {
        // Whitespace
        if (/\s/.test(line[i])) {
            let start = i;
            while (i < line.length && /\s/.test(line[i])) i++;
            tokens.push({ type: "text", value: line.slice(start, i) });
            continue;
        }

        // Line comment
        if (line[i] === '/' && line[i + 1] === '/') {
            tokens.push({ type: "comment", value: line.slice(i) });
            break;
        }

        // String literal
        if (line[i] === '"') {
            let j = i + 1;
            while (j < line.length && line[j] !== '"') {
                if (line[j] === '\\') j++;
                j++;
            }
            tokens.push({ type: "string", value: line.slice(i, j + 1) });
            i = j + 1;
            continue;
        }

        // Char literal
        if (line[i] === "'") {
            let j = i + 1;
            while (j < line.length && line[j] !== "'") {
                if (line[j] === '\\') j++;
                j++;
            }
            tokens.push({ type: "string", value: line.slice(i, j + 1) });
            i = j + 1;
            continue;
        }

        // Hex number
        if (line[i] === '0' && (line[i + 1] === 'x' || line[i + 1] === 'X')) {
            let j = i + 2;
            while (j < line.length && /[0-9a-fA-F]/.test(line[j])) j++;
            // Include optional suffix (U, L, LL)
            while (j < line.length && /[uUlL]/.test(line[j])) j++;
            tokens.push({ type: "number", value: line.slice(i, j) });
            i = j;
            continue;
        }

        // Decimal number
        if (/[0-9]/.test(line[i])) {
            let j = i;
            while (j < line.length && /[0-9]/.test(line[j])) j++;
            while (j < line.length && /[uUlL]/.test(line[j])) j++;
            tokens.push({ type: "number", value: line.slice(i, j) });
            i = j;
            continue;
        }

        // Identifiers and keywords
        if (/[a-zA-Z_]/.test(line[i])) {
            let j = i;
            while (j < line.length && /[a-zA-Z0-9_]/.test(line[j])) j++;
            const word = line.slice(i, j);
            if (KEYWORDS.has(word)) {
                tokens.push({ type: "keyword", value: word });
            } else if (TYPES.has(word)) {
                tokens.push({ type: "type", value: word });
            } else {
                tokens.push({ type: "symbol", value: word });
            }
            i = j;
            continue;
        }

        // Operators and punctuation
        if ("(){}[]".includes(line[i])) {
            tokens.push({ type: "paren", value: line[i] });
            i++;
            continue;
        }

        if ("+-*/%=!<>&|^~?:,.;".includes(line[i])) {
            tokens.push({ type: "operator", value: line[i] });
            i++;
            continue;
        }

        tokens.push({ type: "text", value: line[i] });
        i++;
    }

    return tokens;
}

export default function DecompileView({
    code,
    functionName,
    onSymbolClick,
    onRename,
}: DecompileViewProps) {
    const [hoveredSymbol, setHoveredSymbol] = useState<string | null>(null);
    const [copied, setCopied] = useState(false);
    const copyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    const handleCopy = useCallback(() => {
        if (!code) return;
        navigator.clipboard.writeText(code).then(() => {
            setCopied(true);
            if (copyTimerRef.current) clearTimeout(copyTimerRef.current);
            copyTimerRef.current = setTimeout(() => setCopied(false), 1500);
        });
    }, [code]);

    const lines = useMemo(() => {
        if (!code) return [];
        return code.split("\n").map((line, idx) => ({
            number: idx + 1,
            tokens: tokenize(line),
        }));
    }, [code]);

    const handleSymbolClick = useCallback(
        (symbol: string) => {
            onSymbolClick?.(symbol);
        },
        [onSymbolClick],
    );

    const handleSymbolDoubleClick = useCallback(
        (symbol: string) => {
            onRename?.(symbol);
        },
        [onRename],
    );

    if (!code) {
        return (
            <div className="decomp-view decomp-view--empty">
                <div className="decomp-view__placeholder">Select a function to decompile</div>
            </div>
        );
    }

    return (
        <div className="decomp-view">
            {functionName && (
                <div className="decomp-view__header">
                    <span className="decomp-view__func-name">{functionName}</span>
                    <button
                        className="decomp-view__copy-btn"
                        onClick={handleCopy}
                        title="Copy decompiled code"
                    >
                        {copied ? "✓ Copied" : "📋 Copy"}
                    </button>
                </div>
            )}
            <div className="decomp-view__code">
                {lines.map((line) => (
                    <div key={line.number} className="decomp-line">
                        <span className="decomp-line__number">{line.number}</span>
                        <span className="decomp-line__content">
                            {line.tokens.map((token, ti) => {
                                if (token.type === "symbol") {
                                    const isHovered = hoveredSymbol === token.value;
                                    return (
                                        <span
                                            key={ti}
                                            className={`decomp-token decomp-token--symbol ${isHovered ? "decomp-token--highlight" : ""}`}
                                            onMouseEnter={() => setHoveredSymbol(token.value)}
                                            onMouseLeave={() => setHoveredSymbol(null)}
                                            onClick={() => handleSymbolClick(token.value)}
                                            onDoubleClick={() => handleSymbolDoubleClick(token.value)}
                                            title="Click to highlight all · Double-click to rename"
                                        >
                                            {token.value}
                                        </span>
                                    );
                                }
                                return (
                                    <span key={ti} className={`decomp-token decomp-token--${token.type}`}>
                                        {token.value}
                                    </span>
                                );
                            })}
                        </span>
                    </div>
                ))}
            </div>
        </div>
    );
}
