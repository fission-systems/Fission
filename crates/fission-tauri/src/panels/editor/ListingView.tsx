import React, { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ListingInfo, ListingRow } from "../../types";

interface Props {
    binaryLoaded: boolean;
    onLog: (msg: string) => void;
}

const CHUNK_SIZE = 300;

// ---------------------------------------------------------------------------
// Operand tokenizer — splits Intel-syntax operands into typed spans.
// Inspired by x64dbg's ZydisTokenizer approach.
// ---------------------------------------------------------------------------
type TokType = "reg" | "imm" | "label" | "mem-bracket" | "mem-size" | "seg" | "punc" | "text";

interface Token { type: TokType; text: string }

const REGISTERS = new Set([
    // 64-bit
    "rax","rbx","rcx","rdx","rsi","rdi","rbp","rsp",
    "r8","r9","r10","r11","r12","r13","r14","r15",
    // 32-bit
    "eax","ebx","ecx","edx","esi","edi","ebp","esp",
    "r8d","r9d","r10d","r11d","r12d","r13d","r14d","r15d",
    // 16-bit
    "ax","bx","cx","dx","si","di","bp","sp",
    "r8w","r9w","r10w","r11w","r12w","r13w","r14w","r15w",
    // 8-bit
    "al","bl","cl","dl","ah","bh","ch","dh","sil","dil","bpl","spl",
    "r8b","r9b","r10b","r11b","r12b","r13b","r14b","r15b",
    // FPU/SIMD
    "st0","st1","st2","st3","st4","st5","st6","st7",
    "mm0","mm1","mm2","mm3","mm4","mm5","mm6","mm7",
    "xmm0","xmm1","xmm2","xmm3","xmm4","xmm5","xmm6","xmm7",
    "xmm8","xmm9","xmm10","xmm11","xmm12","xmm13","xmm14","xmm15",
    "ymm0","ymm1","ymm2","ymm3","ymm4","ymm5","ymm6","ymm7",
    "ymm8","ymm9","ymm10","ymm11","ymm12","ymm13","ymm14","ymm15",
    "zmm0","zmm1","zmm2","zmm3","zmm4","zmm5","zmm6","zmm7",
    // Segment
    "cs","ds","es","fs","gs","ss",
    // Control / debug
    "cr0","cr2","cr3","cr4","dr0","dr1","dr2","dr3","dr6","dr7",
    "rip","eip","rflags","eflags",
]);

const MEM_SIZES = new Set([
    "byte", "word", "dword", "qword", "xmmword", "ymmword", "zmmword",
    "tbyte", "oword", "ptr", "fword",
]);

const SEGMENTS = new Set(["cs","ds","es","fs","gs","ss"]);

function tokenizeOperands(raw: string): Token[] {
    if (!raw) return [];
    // Simple regex-based tokenizer to split operands into typed tokens
    const RE = /([a-zA-Z_][a-zA-Z0-9_]*)|(\b0x[0-9a-fA-F]+\b)|(\b[0-9][0-9a-fA-F]*h?\b)|([\[\]])|([,+\-*:])|(\s+)/g;
    const tokens: Token[] = [];
    let lastIndex = 0;

    for (const m of raw.matchAll(RE)) {
        // Emit any unmatched characters as plain text
        if (m.index! > lastIndex) {
            tokens.push({ type: "text", text: raw.slice(lastIndex, m.index!) });
        }
        lastIndex = m.index! + m[0].length;

        const word = m[0];
        const lower = word.toLowerCase();

        if (m[6]) {
            // whitespace — keep as-is
            tokens.push({ type: "text", text: word });
        } else if (m[4]) {
            // brackets [ ]
            tokens.push({ type: "mem-bracket", text: word });
        } else if (m[5]) {
            // punctuation: , + - * :
            if (lower === ":") {
                // segment separator
                tokens.push({ type: "seg", text: word });
            } else {
                tokens.push({ type: "punc", text: word });
            }
        } else if (m[2] || m[3]) {
            // hex or decimal number → immediate
            tokens.push({ type: "imm", text: word });
        } else if (m[1]) {
            // identifier
            if (MEM_SIZES.has(lower)) {
                tokens.push({ type: "mem-size", text: word });
            } else if (SEGMENTS.has(lower)) {
                tokens.push({ type: "seg", text: word });
            } else if (REGISTERS.has(lower)) {
                tokens.push({ type: "reg", text: word });
            } else {
                // Function/label name  (e.g.  call sub_401000)
                tokens.push({ type: "label", text: word });
            }
        }
    }
    // Trailing text
    if (lastIndex < raw.length) {
        tokens.push({ type: "text", text: raw.slice(lastIndex) });
    }
    return tokens;
}

function renderTokens(tokens: Token[]): React.ReactNode[] {
    return tokens.map((t, i) => {
        if (t.type === "text") return <span key={i}>{t.text}</span>;
        return <span key={i} className={`listing-tok--${t.type}`}>{t.text}</span>;
    });
}

export const ListingView: React.FC<Props> = ({ binaryLoaded, onLog }) => {
    const [info, setInfo] = useState<ListingInfo | null>(null);
    const [rows, setRows] = useState<ListingRow[]>([]);
    const [loading, setLoading] = useState(false);
    const [exhausted, setExhausted] = useState(false);
    const [jumpInput, setJumpInput] = useState("");
    const [error, setError] = useState<string | null>(null);

    // Address of the next chunk to load (track per-instruction, avoiding label rows)
    const nextAddrRef = useRef<string | null>(null);
    const sentinelRef = useRef<HTMLDivElement>(null);
    const containerRef = useRef<HTMLDivElement>(null);

    // -------------------------------------------------------------------------
    // Load a chunk of rows from backend
    // -------------------------------------------------------------------------
    const loadChunk = useCallback(async (startAddress: string, reset = false) => {
        if (loading) return;
        setLoading(true);

        try {
            const chunk = await invoke<ListingRow[]>("get_listing_chunk", {
                startAddress,
                count: CHUNK_SIZE,
            });

            if (chunk.length === 0) {
                setExhausted(true);
            } else {
                // Find the last instruction row to track next fetch address
                const lastInsn = [...chunk].reverse().find((r) => r.row_type === "instruction");
                if (lastInsn) {
                    // Next chunk starts one byte after the last instruction — we'll decode
                    // from there and the decoder will align naturally.
                    // We pass the address of the last instruction + 1 byte as hint.
                    // Backend finds the section that contains it.
                    const lastAddr = BigInt(lastInsn.address);
                    nextAddrRef.current = `0x${(lastAddr + 1n).toString(16)}`;
                } else {
                    setExhausted(true);
                }

                if (reset) {
                    setRows(chunk);
                } else {
                    setRows((prev) => [...prev, ...chunk]);
                }
            }
        } catch (e) {
            const msg = String(e);
            onLog(`[Listing] Chunk load error: ${msg}`);
        } finally {
            setLoading(false);
        }
    }, [loading, onLog]);

    // -------------------------------------------------------------------------
    // Bootstrap: fetch listing info, then load first chunk.
    // bootstrapRef lets the binaryLoaded effect always call the latest version
    // without depending on bootstrap (which changes whenever loading flips).
    // -------------------------------------------------------------------------
    const bootstrapRef = useRef<(startAddr?: string) => Promise<void>>(async () => {});

    const bootstrap = useCallback(async (startAddr?: string) => {
        setError(null);
        setRows([]);
        setExhausted(false);
        nextAddrRef.current = null;

        try {
            const meta = await invoke<ListingInfo>("get_listing_info");
            setInfo(meta);

            const resolvedStart = startAddr ?? meta.first_addr;
            await loadChunk(resolvedStart, true /* reset */);
        } catch (e) {
            const msg = String(e);
            setError(msg);
            onLog(`[Listing] Error: ${msg}`);
        }
    }, [loadChunk, onLog]);

    // Keep the ref current so binaryLoaded effect always calls the latest version
    useEffect(() => { bootstrapRef.current = bootstrap; }, [bootstrap]);

    // -------------------------------------------------------------------------
    // IntersectionObserver: load more when sentinel is visible
    // -------------------------------------------------------------------------
    useEffect(() => {
        const sentinel = sentinelRef.current;
        if (!sentinel) return;

        const observer = new IntersectionObserver(
            (entries) => {
                if (entries[0].isIntersecting && !loading && !exhausted && nextAddrRef.current) {
                    loadChunk(nextAddrRef.current);
                }
            },
            { threshold: 0.1 }
        );

        observer.observe(sentinel);
        return () => observer.disconnect();
    }, [loading, exhausted, loadChunk]);

    // -------------------------------------------------------------------------
    // Re-bootstrap when binary is loaded/unloaded
    // -------------------------------------------------------------------------
    useEffect(() => {
        if (binaryLoaded) {
            bootstrapRef.current();
        } else {
            setInfo(null);
            setRows([]);
            setError(null);
            setExhausted(false);
            nextAddrRef.current = null;
        }
    }, [binaryLoaded]);

    // -------------------------------------------------------------------------
    // Jump to address
    // -------------------------------------------------------------------------
    const handleJump = async () => {
        const trimmed = jumpInput.trim();
        if (!trimmed) return;

        const addr = trimmed.startsWith("0x") || trimmed.startsWith("0X")
            ? trimmed
            : `0x${trimmed}`;

        try {
            BigInt(addr); // validate
        } catch {
            onLog(`[Listing] Invalid address: ${trimmed}`);
            return;
        }

        await bootstrap(addr);
        containerRef.current?.scrollTo({ top: 0 });
    };

    // -------------------------------------------------------------------------
    // Render
    // -------------------------------------------------------------------------
    if (!binaryLoaded) {
        return (
            <div className="listing-empty">
                <span className="listing-empty__icon">📄</span>
                <p>No binary loaded</p>
            </div>
        );
    }

    if (error) {
        return (
            <div className="listing-empty listing-empty--error">
                <span className="listing-empty__icon">⚠</span>
                <p>{error}</p>
            </div>
        );
    }

    return (
        <div className="listing-view">
            {/* Toolbar */}
            <div className="listing-view__toolbar">
                {info && (
                    <span className="listing-view__meta">
                        EP: {info.entry_point} | {info.first_addr} – {info.last_addr}
                        &nbsp;({(Number(info.total_exec_bytes) / 1024).toFixed(1)} KB executable)
                    </span>
                )}
                <div className="listing-view__jump">
                    <input
                        className="listing-view__jump-input"
                        placeholder="Jump to address…"
                        value={jumpInput}
                        onChange={(e) => setJumpInput(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && handleJump()}
                    />
                    <button className="listing-view__jump-btn" onClick={handleJump}>
                        Go
                    </button>
                </div>
            </div>

            {/* Row list */}
            <div className="listing-view__body" ref={containerRef}>
                <table className="listing-view__table">
                    <tbody>
                        {rows.map((row, i) =>
                            row.row_type === "label" ? (
                                <tr key={`lbl-${i}`} className="listing-row listing-row--label">
                                    <td colSpan={4} className="listing-row__label">
                                        {row.label}
                                    </td>
                                </tr>
                            ) : (
                                <tr key={`ins-${i}`} className="listing-row listing-row--insn">
                                    <td className="listing-row__addr">{row.address}</td>
                                    <td className="listing-row__bytes">{row.bytes}</td>
                                    <td className={`listing-row__mnem listing-row__mnem--${row.mnemonic_type || "normal"}`}>
                                        {row.mnemonic}
                                    </td>
                                    <td className="listing-row__ops">
                                        {renderTokens(tokenizeOperands(row.operands))}
                                        {row.comment && (
                                            <span className="listing-row__comment">
                                                ; {row.comment}
                                            </span>
                                        )}
                                    </td>
                                </tr>
                            )
                        )}
                    </tbody>
                </table>

                {/* Infinite-scroll sentinel */}
                <div ref={sentinelRef} className="listing-view__sentinel">
                    {loading && <span className="listing-view__loading">Loading…</span>}
                    {exhausted && !loading && (
                        <span className="listing-view__end">— end of section —</span>
                    )}
                </div>
            </div>
        </div>
    );
};

export default ListingView;
