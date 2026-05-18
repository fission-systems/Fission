import { useState, useMemo, useCallback, useRef } from "react";
import type { DecompileResult } from "../../types";

interface DecompileViewProps {
    result: DecompileResult | null;
    status?: "idle" | "loading" | "ready" | "error";
    onSymbolClick?: (symbol: string) => void;
    onRename?: (symbol: string) => void;
}

// Simple C-like tokenizer for syntax highlighting
interface Token {
    type: "keyword" | "type" | "number" | "string" | "comment" | "symbol" | "operator" | "paren" | "text";
    value: string;
}

function isUnstructuredPreviewCode(code: string | undefined): boolean {
    if (!code) return false;
    if (code.includes("goto ")) return true;
    return code.split("\n").some((line) => {
        const trimmed = line.trim();
        return trimmed.endsWith(":") && !trimmed.startsWith("case ") && trimmed !== "default:";
    });
}

function decompileStatusLabel(result: DecompileResult): string {
    const reason = result.fallback_reason ?? "";
    if (reason.startsWith("assembly_fallback:")) {
        return "Fission NIR -> Assembly fallback";
    }
    if (reason.startsWith("preview_timeout:") || reason.startsWith("nir_timeout:")) {
        return "Fission NIR -> Timeout";
    }
    if (reason.startsWith("preview_unsupported:") || reason.startsWith("nir_unsupported:")) {
        return "Fission NIR -> Unsupported";
    }
    if (reason.startsWith("legacy_fallback:")) {
        return "Fission NIR -> Fallback";
    }
    if (reason.startsWith("native_pcode_failure:")) {
        return "Fission NIR -> p-code failure";
    }
    if (result.engine_used === "nir") {
        if (!result.fell_back && isUnstructuredPreviewCode(result.code)) {
            return "Fission NIR (unstructured)";
        }
        return result.fell_back ? "Fission NIR rescue" : "Fission NIR";
    }
    return "Fission NIR";
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
    "RECT", "LPRECT", "POINT", "LPPOINT", "MSG", "LPMSG", "WSADATA", "LPWSADATA",
    "HWND", "HDC", "HMENU", "WPARAM", "LPARAM", "LRESULT", "ULONG_PTR", "DWORD_PTR",
]);

function isLikelyTypeToken(word: string): boolean {
    return TYPES.has(word) || /^(?:LP|P)?[A-Z][A-Z0-9_]{2,}$/.test(word);
}

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
            } else if (isLikelyTypeToken(word)) {
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
    result,
    status = "idle",
    onSymbolClick,
    onRename,
}: DecompileViewProps) {
    const code = result?.code ?? null;
    const [hoveredSymbol, setHoveredSymbol] = useState<string | null>(null);
    const [selectedSymbol, setSelectedSymbol] = useState<string | null>(null);
    const [copied, setCopied] = useState(false);
    const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
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
    const diagnostics = result?.diagnostics ?? null;
    const diagnosticChips = useMemo(() => {
        if (!diagnostics) return [];
        const chips = [
            ["decode", diagnostics.decode.stop_reason || "ok"],
            ["attempts", diagnostics.decode.attempts.toString()],
            ["p-code", (diagnostics.decode.raw_pcode_ops ?? 0).toString()],
            ["blocks", (diagnostics.decode.raw_pcode_blocks ?? 0).toString()],
        ];
        if (diagnostics.nir) {
            chips.push(
                ["build", `${diagnostics.nir.build_duration_ms}ms`],
                ["normalize", `${diagnostics.nir.normalize_duration_ms}ms`],
                ["structure", `${diagnostics.nir.structuring_duration_ms}ms`],
                ["render", `${diagnostics.nir.render_duration_ms}ms`],
            );
        }
        return chips;
    }, [diagnostics]);

    const handleSymbolClick = useCallback(
        (symbol: string) => {
            setSelectedSymbol((prev) => (prev === symbol ? null : symbol));
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

    if (!code && status === "loading") {
        return (
            <div className="decomp-view decomp-view--empty">
                <div className="decomp-view__placeholder">Decompiling function...</div>
            </div>
        );
    }

    if (!code) {
        return (
            <div className="decomp-view decomp-view--empty">
                <div className="decomp-view__placeholder">Select a function to decompile</div>
            </div>
        );
    }

    return (
        <div className="decomp-view">
            {result && (
                <div className="decomp-view__header">
                    <div className="decomp-view__header-left">
                        <span className="decomp-view__func-name">{result.function_name}</span>
                        <span className="decomp-view__func-name">{result.address}</span>
                        <span className="decomp-view__func-name">{decompileStatusLabel(result)}</span>
                    </div>
                    <div className="decomp-view__header-actions">
                        {diagnostics && (
                            <button
                                className="decomp-view__copy-btn"
                                onClick={() => setDiagnosticsOpen((open) => !open)}
                                title="Show decompiler diagnostics"
                            >
                                Diagnostics
                            </button>
                        )}
                        <button
                            className="decomp-view__copy-btn"
                            onClick={handleCopy}
                            title="Copy decompiled code"
                        >
                            {copied ? "Copied" : "Copy"}
                        </button>
                    </div>
                </div>
            )}
            {diagnostics && (
                <div className="decomp-view__diagnostics">
                    <div className="decomp-view__diag-chip-row">
                        {diagnosticChips.map(([label, value]) => (
                            <span key={label} className="decomp-view__diag-chip">
                                <label>{label}</label>
                                <strong>{value}</strong>
                            </span>
                        ))}
                    </div>
                    {diagnosticsOpen && (
                        <div className="decomp-view__diag-grid">
                            <div className="decomp-view__diag-section">
                                <h4>Decode</h4>
                                <dl>
                                    <dt>entry</dt><dd>{diagnostics.decode.entry_address}</dd>
                                    <dt>max bytes</dt><dd>{diagnostics.decode.max_bytes}</dd>
                                    <dt>instruction limit</dt><dd>{diagnostics.decode.instruction_limit}</dd>
                                    <dt>edges</dt><dd>{diagnostics.decode.raw_pcode_edges ?? 0}</dd>
                                    <dt>strict retry</dt><dd>{diagnostics.decode.strict_indirect_retry_attempted ? "yes" : "no"}</dd>
                                    <dt>wrapper probe</dt><dd>{diagnostics.decode.wrapper_probe_matched ? "matched" : diagnostics.decode.wrapper_probe_attempted ? "attempted" : "off"}</dd>
                                </dl>
                            </div>
                            {diagnostics.nir && (
                                <div className="decomp-view__diag-section">
                                    <h4>NIR</h4>
                                    <dl>
                                        <dt>validated ops</dt><dd>{diagnostics.nir.validated_pcode_op_count}</dd>
                                        <dt>invalid shapes</dt><dd>{diagnostics.nir.invalid_pcode_shape_count}</dd>
                                        <dt>irreducible SCC</dt><dd>{diagnostics.nir.structuring_irreducible_scc_count}</dd>
                                        <dt>emit-ready failed</dt><dd>{diagnostics.nir.region_emit_ready_failed_count}</dd>
                                        <dt>typed facts</dt><dd>{diagnostics.nir.typed_fact_evidence_count}</dd>
                                        <dt>typed conflicts</dt><dd>{diagnostics.nir.typed_fact_conflict_count}</dd>
                                        <dt>surface facts</dt><dd>{diagnostics.nir.surface_fact_promotion_count}</dd>
                                        <dt>replacement plans</dt><dd>{diagnostics.nir.replacement_plan_completed_count}/{diagnostics.nir.replacement_plan_candidate_count}</dd>
                                    </dl>
                                </div>
                            )}
                            <div className="decomp-view__diag-section">
                                <h4>Pipeline</h4>
                                <dl>
                                    {diagnostics.pipeline_stage_status.map((item) => (
                                        <div key={item.name} className="decomp-view__diag-pair">
                                            <dt>{item.name}</dt><dd>{item.value}</dd>
                                        </div>
                                    ))}
                                </dl>
                            </div>
                            <div className="decomp-view__diag-section">
                                <h4>Sources</h4>
                                <dl>
                                    {diagnostics.template_sources.map((item) => (
                                        <div key={item.name} className="decomp-view__diag-pair">
                                            <dt>{item.name}</dt><dd>{item.value}</dd>
                                        </div>
                                    ))}
                                    {diagnostics.terminal_opcodes.map((item) => (
                                        <div key={`term-${item.name}`} className="decomp-view__diag-pair">
                                            <dt>{item.name}</dt><dd>{item.value}</dd>
                                        </div>
                                    ))}
                                </dl>
                            </div>
                        </div>
                    )}
                </div>
            )}
            <div className="decomp-view__code">
                {lines.map((line) => (
                    <div key={line.number} className="decomp-line">
                        <span className="decomp-line__number">{line.number}</span>
                        <span className="decomp-line__content">
                            {line.tokens.map((token, ti) => {
                                if (token.type === "symbol") {
                                    const isHighlighted =
                                        hoveredSymbol === token.value || selectedSymbol === token.value;
                                    return (
                                        <span
                                            key={ti}
                                            className={`decomp-token decomp-token--symbol ${isHighlighted ? "decomp-token--highlight" : ""}`}
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
