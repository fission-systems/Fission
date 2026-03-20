// Fission — Decompiler Options Dialog (Ghidra-level configuration).
//
// Tab-based modal with categories: Analysis, Post-Processing, Display, Performance.
// Apply button sends options to backend, clears decompiler cache, and triggers re-decompilation.

import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { DecompilerOptions } from "../../types";
import { defaultDecompilerOptions } from "../../types";

interface DecompilerOptionsDialogProps {
    open: boolean;
    onClose: () => void;
    onApplied: () => void;
    onLog: (msg: string) => void;
}

type TabId = "analysis" | "postprocess" | "display" | "performance";
type DecompilerOptionsCategory = Exclude<keyof DecompilerOptions, "engine_mode">;

interface ToggleRowProps {
    label: string;
    description?: string;
    checked: boolean;
    onChange: (v: boolean) => void;
}

function ToggleRow({ label, description, checked, onChange }: ToggleRowProps) {
    return (
        <div className="decomp-opts__row">
            <div className="decomp-opts__row-text">
                <span className="decomp-opts__row-label">{label}</span>
                {description && <span className="decomp-opts__row-desc">{description}</span>}
            </div>
            <label className="decomp-opts__toggle">
                <input
                    type="checkbox"
                    checked={checked}
                    onChange={(e) => onChange(e.target.checked)}
                />
                <span className="decomp-opts__toggle-slider" />
            </label>
        </div>
    );
}

interface NumberRowProps {
    label: string;
    description?: string;
    value: number;
    min: number;
    max: number;
    step?: number;
    onChange: (v: number) => void;
}

function NumberRow({ label, description, value, min, max, step, onChange }: NumberRowProps) {
    return (
        <div className="decomp-opts__row">
            <div className="decomp-opts__row-text">
                <span className="decomp-opts__row-label">{label}</span>
                {description && <span className="decomp-opts__row-desc">{description}</span>}
            </div>
            <input
                type="number"
                className="decomp-opts__number-input"
                value={value}
                min={min}
                max={max}
                step={step ?? 1}
                onChange={(e) => onChange(Number(e.target.value))}
            />
        </div>
    );
}

interface SelectRowProps {
    label: string;
    description?: string;
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
}

function SelectRow({ label, description, value, options, onChange }: SelectRowProps) {
    return (
        <div className="decomp-opts__row">
            <div className="decomp-opts__row-text">
                <span className="decomp-opts__row-label">{label}</span>
                {description && <span className="decomp-opts__row-desc">{description}</span>}
            </div>
            <select
                className="decomp-opts__select"
                value={value}
                onChange={(e) => onChange(e.target.value)}
            >
                {options.map((o) => (
                    <option key={o.value} value={o.value}>{o.label}</option>
                ))}
            </select>
        </div>
    );
}

const TABS: { id: TabId; label: string }[] = [
    { id: "analysis", label: "Analysis" },
    { id: "postprocess", label: "Post-Processing" },
    { id: "display", label: "Display" },
    { id: "performance", label: "Performance" },
];

export default function DecompilerOptionsDialog({
    open,
    onClose,
    onApplied,
    onLog,
}: DecompilerOptionsDialogProps) {
    const [activeTab, setActiveTab] = useState<TabId>("analysis");
    const [options, setOptions] = useState<DecompilerOptions>(defaultDecompilerOptions());
    const [loading, setLoading] = useState(false);
    const [dirty, setDirty] = useState(false);

    // Load current options when dialog opens
    useEffect(() => {
        if (!open) return;
        invoke<DecompilerOptions>("get_decompiler_options")
            .then((opts) => {
                setOptions(opts);
                setDirty(false);
            })
            .catch((e) => onLog(`Failed to load decompiler options: ${e}`));
    }, [open, onLog]);

    const update = useCallback(
        <K extends DecompilerOptionsCategory>(
            category: K,
            field: string,
            value: boolean | number | string,
        ) => {
            setOptions((prev) => ({
                ...prev,
                [category]: { ...prev[category], [field]: value },
            }));
            setDirty(true);
        },
        [],
    );

    const handleApply = useCallback(async () => {
        setLoading(true);
        try {
            await invoke("apply_decompiler_options", { options });
            onLog("Decompiler options applied — cache cleared.");
            setDirty(false);
            onApplied();
        } catch (e) {
            onLog(`Failed to apply decompiler options: ${e}`);
        } finally {
            setLoading(false);
        }
    }, [options, onLog, onApplied]);

    const handleReset = useCallback(() => {
        setOptions(defaultDecompilerOptions());
        setDirty(true);
    }, []);

    if (!open) return null;

    return (
        <div className="dialog-overlay" onClick={onClose}>
            <div
                className="dialog decomp-opts-dialog"
                onClick={(e) => e.stopPropagation()}
            >
                {/* Header */}
                <div className="decomp-opts__header">
                    <span className="decomp-opts__title">Decompiler Options</span>
                    <button className="decomp-opts__close" onClick={onClose}>×</button>
                </div>

                {/* Tabs */}
                <div className="decomp-opts__tabs">
                    {TABS.map((tab) => (
                        <button
                            key={tab.id}
                            className={`decomp-opts__tab ${activeTab === tab.id ? "decomp-opts__tab--active" : ""}`}
                            onClick={() => setActiveTab(tab.id)}
                        >
                            {tab.label}
                        </button>
                    ))}
                </div>

                {/* Tab Content */}
                <div className="decomp-opts__content">
                    {activeTab === "analysis" && (
                        <div className="decomp-opts__section">
                            <div className="decomp-opts__section-title">Ghidra Engine Analysis</div>
                            <SelectRow
                                label="Decompiler Engine"
                                description="Choose Fission NIR directly or keep NIR-first auto routing"
                                value={options.engine_mode}
                                options={[
                                    { value: "auto", label: "Auto (Fission NIR when safe)" },
                                    { value: "nir", label: "Fission NIR" },
                                    { value: "mlil_preview", label: "MLIL Preview (Deprecated)" },
                                ]}
                                onChange={(v) => {
                                    setOptions((prev) => ({
                                        ...prev,
                                        engine_mode: v as DecompilerOptions["engine_mode"],
                                    }));
                                    setDirty(true);
                                }}
                            />
                            <ToggleRow
                                label="Infer Constant Pointers"
                                description="Treat constants as pointers when plausible"
                                checked={options.analysis.infer_pointers}
                                onChange={(v) => update("analysis", "infer_pointers", v)}
                            />
                            <ToggleRow
                                label="Recover For-Loops"
                                description="Reconstruct for-loop structures from while loops"
                                checked={options.analysis.analyze_loops}
                                onChange={(v) => update("analysis", "analyze_loops", v)}
                            />
                            <ToggleRow
                                label="Read-Only Propagation"
                                description="Treat read-only memory as constant values"
                                checked={options.analysis.readonly_propagate}
                                onChange={(v) => update("analysis", "readonly_propagate", v)}
                            />
                            <ToggleRow
                                label="Record Jump Table Loads"
                                description="Detect and recover jump table targets"
                                checked={options.analysis.record_jumploads}
                                onChange={(v) => update("analysis", "record_jumploads", v)}
                            />
                            <ToggleRow
                                label="Allow Inline Functions"
                                description="Permit function inlining during decompilation"
                                checked={options.analysis.allow_inline}
                                onChange={(v) => update("analysis", "allow_inline", v)}
                            />
                            <ToggleRow
                                label="Disable Too-Many-Instructions Error"
                                description="Skip error when function exceeds instruction limit"
                                checked={options.analysis.disable_toomanyinstructions_error}
                                onChange={(v) => update("analysis", "disable_toomanyinstructions_error", v)}
                            />
                        </div>
                    )}

                    {activeTab === "postprocess" && (
                        <>
                            <div className="decomp-opts__section">
                                <div className="decomp-opts__section-title">C++ Post-Processing (Ghidra Output)</div>
                                <ToggleRow label="Apply Struct Definitions" description="Replace raw offsets with struct field names" checked={options.cpp_postprocess.apply_struct_definitions} onChange={(v) => update("cpp_postprocess", "apply_struct_definitions", v)} />
                                <ToggleRow label="IAT Symbol Resolution" description="Replace import table addresses with function names" checked={options.cpp_postprocess.iat_symbols} onChange={(v) => update("cpp_postprocess", "iat_symbols", v)} />
                                <ToggleRow label="Strip Shadow Parameters" description="Remove x64 shadow space parameters" checked={options.cpp_postprocess.strip_shadow_params} onChange={(v) => update("cpp_postprocess", "strip_shadow_params", v)} />
                                <ToggleRow label="Smart Constants" description="Replace magic numbers with named constants" checked={options.cpp_postprocess.smart_constants} onChange={(v) => update("cpp_postprocess", "smart_constants", v)} />
                                <ToggleRow label="Inline String Literals" description="Replace string addresses with inline literals" checked={options.cpp_postprocess.inline_strings} onChange={(v) => update("cpp_postprocess", "inline_strings", v)} />
                                <ToggleRow label="Constant Folding" description="Simplify constant expressions" checked={options.cpp_postprocess.constants} onChange={(v) => update("cpp_postprocess", "constants", v)} />
                                <ToggleRow label="GUID Recognition" description="Identify and label Windows GUIDs" checked={options.cpp_postprocess.guids} onChange={(v) => update("cpp_postprocess", "guids", v)} />
                                <ToggleRow label="Unicode String Detection" description="Detect and format wide/UTF-16 strings" checked={options.cpp_postprocess.unicode_strings} onChange={(v) => update("cpp_postprocess", "unicode_strings", v)} />
                                <ToggleRow label="Interlocked Pattern Recognition" description="Identify atomic/interlocked operations" checked={options.cpp_postprocess.interlocked_patterns} onChange={(v) => update("cpp_postprocess", "interlocked_patterns", v)} />
                                <ToggleRow label="Unknown Type Cleanup" description="Replace xunknown types with readable names" checked={options.cpp_postprocess.xunknown_types} onChange={(v) => update("cpp_postprocess", "xunknown_types", v)} />
                                <ToggleRow label="SEH Cleanup" description="Clean up structured exception handling boilerplate" checked={options.cpp_postprocess.seh_cleanup} onChange={(v) => update("cpp_postprocess", "seh_cleanup", v)} />
                                <ToggleRow label="Global Symbol Resolution" description="Replace global addresses with symbol names" checked={options.cpp_postprocess.global_symbols} onChange={(v) => update("cpp_postprocess", "global_symbols", v)} />
                                <ToggleRow label="Internal Name Cleanup" description="Replace internal compiler-generated names" checked={options.cpp_postprocess.internal_names} onChange={(v) => update("cpp_postprocess", "internal_names", v)} />
                                <ToggleRow label="Struct Offset Labels" description="Label struct member offsets" checked={options.cpp_postprocess.struct_offsets} onChange={(v) => update("cpp_postprocess", "struct_offsets", v)} />
                                <ToggleRow label="FID Name Application" description="Apply function identification names" checked={options.cpp_postprocess.fid_names} onChange={(v) => update("cpp_postprocess", "fid_names", v)} />
                            </div>
                            <div className="decomp-opts__section">
                                <div className="decomp-opts__section-title">Rust Post-Processing (Code Cleanup)</div>
                                <ToggleRow label="Clean Rust Boilerplate" description="Remove Rust safety-check overhead code" checked={options.rust_postprocess.clean_rust} onChange={(v) => update("rust_postprocess", "clean_rust", v)} />
                                <ToggleRow label="Clean Go Boilerplate" description="Remove Go runtime boilerplate code" checked={options.rust_postprocess.clean_go} onChange={(v) => update("rust_postprocess", "clean_go", v)} />
                                <ToggleRow label="Swift Symbol Demangling" description="Demangle Swift mangled symbol names" checked={options.rust_postprocess.swift_demangle} onChange={(v) => update("rust_postprocess", "swift_demangle", v)} />
                                <ToggleRow label="Field Offset Replacement" description="Replace numeric offsets with struct field names" checked={options.rust_postprocess.field_offsets} onChange={(v) => update("rust_postprocess", "field_offsets", v)} />
                                <ToggleRow label="Insert Missing Casts" description="Add type casts for assignment mismatches" checked={options.rust_postprocess.insert_casts} onChange={(v) => update("rust_postprocess", "insert_casts", v)} />
                                <ToggleRow label="Arithmetic Idiom Recovery" description="Recognize multiplication/division idioms" checked={options.rust_postprocess.arithmetic_idioms} onChange={(v) => update("rust_postprocess", "arithmetic_idioms", v)} />
                                <ToggleRow label="Temp Variable Inlining" description="Inline trivial single-use temporary variables" checked={options.rust_postprocess.temp_var_inlining} onChange={(v) => update("rust_postprocess", "temp_var_inlining", v)} />
                                <ToggleRow label="Stack Variable Normalization" description="Rename exposed Stack_ locals into local_* form" checked={options.rust_postprocess.stack_var_normalization} onChange={(v) => update("rust_postprocess", "stack_var_normalization", v)} />
                                <ToggleRow label="Piece Access Normalization" description="Rewrite _offset_size_ piece syntax into explicit pointer casts" checked={options.rust_postprocess.piece_access_normalization} onChange={(v) => update("rust_postprocess", "piece_access_normalization", v)} />
                                <ToggleRow label="Deref → Array Index" description="Convert *(ptr + N) to ptr[N] notation" checked={options.rust_postprocess.deref_to_array} onChange={(v) => update("rust_postprocess", "deref_to_array", v)} />
                                <ToggleRow label="Bitwise → Logical Ops" description="Convert bitwise & | to logical && || in conditions" checked={options.rust_postprocess.bitop_to_logicop} onChange={(v) => update("rust_postprocess", "bitop_to_logicop", v)} />
                                <ToggleRow label="Dead Branch Removal" description="Remove constant-condition dead branches" checked={options.rust_postprocess.remove_dead_branches} onChange={(v) => update("rust_postprocess", "remove_dead_branches", v)} />
                                <ToggleRow label="If Structure Simplification" description="Simplify empty else and if-return patterns" checked={options.rust_postprocess.simplify_if} onChange={(v) => update("rust_postprocess", "simplify_if", v)} />
                                <ToggleRow label="While → For Conversion" description="Convert while loops to for loops" checked={options.rust_postprocess.while_to_for} onChange={(v) => update("rust_postprocess", "while_to_for", v)} />
                                <ToggleRow label="Dead Assignment Removal" description="Remove unused local variable assignments" checked={options.rust_postprocess.dead_assign_removal} onChange={(v) => update("rust_postprocess", "dead_assign_removal", v)} />
                                <ToggleRow label="Induction Variable Naming" description="Name loop counters as i, j, k" checked={options.rust_postprocess.rename_induction_vars} onChange={(v) => update("rust_postprocess", "rename_induction_vars", v)} />
                                <ToggleRow label="Semantic Variable Naming" description="Apply semantic names (argc, argv, result)" checked={options.rust_postprocess.rename_semantic_vars} onChange={(v) => update("rust_postprocess", "rename_semantic_vars", v)} />
                                <ToggleRow label="Loop Idiom Recognition" description="Recognize strlen, memset, popcount patterns" checked={options.rust_postprocess.loop_idioms} onChange={(v) => update("rust_postprocess", "loop_idioms", v)} />
                                <ToggleRow label="Switch Reconstruction" description="Reconstruct switch from if/else chains" checked={options.rust_postprocess.switch_reconstruction} onChange={(v) => update("rust_postprocess", "switch_reconstruction", v)} />
                                <ToggleRow label="Multiply → Shift" description="Convert multiply by power-of-2 to bitshift" checked={options.rust_postprocess.mul_to_shift} onChange={(v) => update("rust_postprocess", "mul_to_shift", v)} />
                                <ToggleRow label="DWARF Name Substitution" description="Apply debug info variable/parameter names" checked={options.rust_postprocess.dwarf_names} onChange={(v) => update("rust_postprocess", "dwarf_names", v)} />
                                <ToggleRow label="String Pointer Replacement" description="Inline discovered string literals for pointer constants" checked={options.rust_postprocess.string_pointers} onChange={(v) => update("rust_postprocess", "string_pointers", v)} />
                            </div>
                        </>
                    )}

                    {activeTab === "display" && (
                        <div className="decomp-opts__section">
                            <div className="decomp-opts__section-title">Output Formatting</div>
                            <NumberRow
                                label="Max Line Width"
                                description="Characters per line before wrapping"
                                value={options.display.max_line_width}
                                min={40} max={200}
                                onChange={(v) => update("display", "max_line_width", v)}
                            />
                            <NumberRow
                                label="Indent Width"
                                description="Number of spaces per indentation level"
                                value={options.display.indent_width}
                                min={1} max={8}
                                onChange={(v) => update("display", "indent_width", v)}
                            />
                            <SelectRow
                                label="Integer Format"
                                description="How integer literals are displayed"
                                value={options.display.integer_format}
                                options={[
                                    { value: "best_fit", label: "Best Fit" },
                                    { value: "hex", label: "Hexadecimal" },
                                    { value: "decimal", label: "Decimal" },
                                ]}
                                onChange={(v) => update("display", "integer_format", v)}
                            />
                            <SelectRow
                                label="Comment Style"
                                description="Comment syntax in output"
                                value={options.display.comment_style}
                                options={[
                                    { value: "c_style", label: "C-style  /* */" },
                                    { value: "cpp_style", label: "C++ style  //" },
                                ]}
                                onChange={(v) => update("display", "comment_style", v)}
                            />
                            <ToggleRow
                                label="Show Type Casts"
                                description="Display explicit type cast expressions"
                                checked={options.display.show_casts}
                                onChange={(v) => update("display", "show_casts", v)}
                            />
                            <ToggleRow
                                label="Show Namespace Qualifiers"
                                description="Display namespace prefixes on symbols"
                                checked={options.display.show_namespaces}
                                onChange={(v) => update("display", "show_namespaces", v)}
                            />
                            <ToggleRow
                                label="Show Line Numbers"
                                description="Display line numbers in decompiled output"
                                checked={options.display.show_line_numbers}
                                onChange={(v) => update("display", "show_line_numbers", v)}
                            />
                        </div>
                    )}

                    {activeTab === "performance" && (
                        <div className="decomp-opts__section">
                            <div className="decomp-opts__section-title">Limits &amp; Performance</div>
                            <NumberRow
                                label="Timeout (ms)"
                                description="Maximum decompilation time per function"
                                value={options.performance.timeout_ms}
                                min={1000} max={300000} step={1000}
                                onChange={(v) => update("performance", "timeout_ms", v)}
                            />
                            <NumberRow
                                label="Max Function Size (bytes)"
                                description="Skip functions larger than this"
                                value={options.performance.max_function_size}
                                min={1024} max={1048576} step={1024}
                                onChange={(v) => update("performance", "max_function_size", v)}
                            />
                            <NumberRow
                                label="Max Instructions"
                                description="Abort decompilation above this count"
                                value={options.performance.max_instructions}
                                min={1000} max={10000000} step={1000}
                                onChange={(v) => update("performance", "max_instructions", v)}
                            />
                            <NumberRow
                                label="Cache Size"
                                description="Number of decompiled functions to cache"
                                value={options.performance.cache_size}
                                min={1} max={1000}
                                onChange={(v) => update("performance", "cache_size", v)}
                            />
                        </div>
                    )}
                </div>

                {/* Actions */}
                <div className="decomp-opts__actions">
                    <button
                        className="dialog__btn"
                        onClick={handleReset}
                        title="Reset all options to defaults"
                    >
                        Reset to Defaults
                    </button>
                    <div className="decomp-opts__actions-right">
                        <button className="dialog__btn" onClick={onClose}>
                            Cancel
                        </button>
                        <button
                            className="dialog__btn dialog__btn--primary"
                            onClick={handleApply}
                            disabled={loading || !dirty}
                            title={dirty ? "Apply changes and re-decompile" : "No changes to apply"}
                        >
                            {loading ? "Applying…" : "Apply"}
                        </button>
                    </div>
                </div>
            </div>
        </div>
    );
}
