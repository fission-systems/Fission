import { useState, useCallback, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save } from "@tauri-apps/plugin-dialog";
import type { HexRow, HexViewData } from "../types";

// Quick patch presets
const QUICK_PATCHES: { label: string; bytes: number[] }[] = [
    { label: "NOP (0x90)", bytes: [0x90] },
    { label: "JE→JNE (0x74→0x75)", bytes: [0x75] },
    { label: "JNE→JE (0x75→0x74)", bytes: [0x74] },
    { label: "JMP Short (0xEB)", bytes: [0xeb] },
    { label: "RET (0xC3)", bytes: [0xc3] },
    { label: "NOP×2", bytes: [0x90, 0x90] },
    { label: "NOP×4", bytes: [0x90, 0x90, 0x90, 0x90] },
    { label: "NOP×6", bytes: [0x90, 0x90, 0x90, 0x90, 0x90, 0x90] },
];

interface HexViewProps {
    /** Controlled mode: display this data (bottom panel) */
    data?: HexViewData | null;
    highlightAddress?: string | null;
    /** Standalone mode: binary is loaded, component fetches its own data */
    binaryLoaded?: boolean;
    /** Initial address to focus on (hex string like "0x401000") */
    initialAddress?: string | null;
    onLog?: (msg: string) => void;
}

const BYTES_PER_PAGE = 512; // 32 rows × 16 bytes

export default function HexView({
    data: propData,
    highlightAddress,
    binaryLoaded,
    initialAddress,
    onLog,
}: HexViewProps) {
    const isStandalone = binaryLoaded !== undefined;

    // Standalone state
    const [standaloneData, setStandaloneData] = useState<HexViewData | null>(null);
    const [currentAddress, setCurrentAddress] = useState(0);
    const [gotoInput, setGotoInput] = useState("");
    const [loadingHex, setLoadingHex] = useState(false);

    // Patch controls
    const [patchOffset, setPatchOffset] = useState("");
    const [patchBytes, setPatchBytes] = useState("");
    const [quickPatch, setQuickPatch] = useState("");
    const [patchStatus, setPatchStatus] = useState<string | null>(null);

    const displayData = isStandalone ? standaloneData : propData;

    const log = (msg: string) => onLog?.(msg);

    const loadAt = useCallback(async (address: number) => {
        if (!binaryLoaded) return;
        setLoadingHex(true);
        try {
            const hex = await invoke<HexViewData>("get_hex_view", {
                address,
                length: BYTES_PER_PAGE,
            });
            setStandaloneData(hex);
            setCurrentAddress(address);
        } catch (e) {
            log(`Hex load error: ${e}`);
        } finally {
            setLoadingHex(false);
        }
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [binaryLoaded]);

    useEffect(() => {
        if (!isStandalone || !binaryLoaded) return;
        const addr = initialAddress
            ? parseInt(initialAddress, 16) || parseInt(initialAddress)
            : 0;
        loadAt(addr);
    // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [binaryLoaded]);

    const handleGoto = () => {
        const raw = gotoInput.trim();
        if (!raw) return;
        const addr = raw.startsWith("0x") || raw.startsWith("0X")
            ? parseInt(raw, 16)
            : parseInt(raw, 16);
        if (isNaN(addr)) { setPatchStatus("Invalid offset"); return; }
        loadAt(addr);
    };

    const handleApplyPatch = async () => {
        const addrStr = patchOffset.trim();
        if (!addrStr) { setPatchStatus("Enter an offset/address"); return; }
        const addr = parseInt(addrStr, 16) || parseInt(addrStr);
        if (isNaN(addr)) { setPatchStatus("Invalid address"); return; }

        let byteArr: number[];
        if (quickPatch) {
            const preset = QUICK_PATCHES.find((p) => p.label === quickPatch);
            byteArr = preset ? [...preset.bytes] : [];
        } else {
            byteArr = patchBytes.trim().split(/\s+/).map((h) => parseInt(h, 16));
        }

        if (byteArr.length === 0 || byteArr.some(isNaN)) {
            setPatchStatus("Invalid bytes (space-separated hex, e.g. 90 90 90)");
            return;
        }
        try {
            const original = await invoke<number[]>("patch_bytes", { address: addr, bytes: byteArr });
            const hexOrig = original.map((b) => b.toString(16).padStart(2, "0")).join(" ");
            const hexNew = byteArr.map((b) => b.toString(16).padStart(2, "0")).join(" ");
            setPatchStatus(`✓ Patched at 0x${addr.toString(16)}: [${hexOrig}] → [${hexNew}]`);
            log(`Patch at 0x${addr.toString(16)}: ${hexNew}`);
            await loadAt(currentAddress);
        } catch (e) {
            setPatchStatus(`Error: ${e}`);
        }
    };

    const handleSaveAs = async () => {
        try {
            const path = await save({
                filters: [{ name: "Binary", extensions: ["exe", "dll", "elf", "so", "bin", "*"] }],
                defaultPath: "patched_binary.exe",
            });
            if (!path) return;
            await invoke("save_patched_binary", { path });
            log(`Saved patched binary → ${path}`);
            setPatchStatus(`✓ Saved to ${path}`);
        } catch (e) {
            setPatchStatus(`Save error: ${e}`);
        }
    };

    if (!displayData || displayData.rows.length === 0) {
        return (
            <div className="hex-view hex-view--empty">
                <div className="hex-view__placeholder">
                    {loadingHex ? "Loading..." : "No hex data loaded"}
                </div>
            </div>
        );
    }

    return (
        <div className="hex-view">
            {/* Navigation bar — standalone editor mode only */}
            {isStandalone && (
                <div className="hex-view__navbar">
                    <input
                        className="hex-view__goto-input"
                        type="text"
                        placeholder="Go to offset (hex)"
                        value={gotoInput}
                        onChange={(e) => setGotoInput(e.target.value)}
                        onKeyDown={(e) => e.key === "Enter" && handleGoto()}
                        spellCheck={false}
                    />
                    <button className="hex-view__btn" onClick={handleGoto}>Go</button>
                    <button
                        className="hex-view__btn"
                        onClick={() => loadAt(Math.max(0, currentAddress - BYTES_PER_PAGE))}
                        disabled={currentAddress === 0}
                    >← Prev</button>
                    <button
                        className="hex-view__btn"
                        onClick={() => loadAt(currentAddress + BYTES_PER_PAGE)}
                    >Next →</button>
                    <span className="hex-view__offset-label">
                        {loadingHex ? "Loading…" : `@ 0x${currentAddress.toString(16).padStart(8, "0")}`}
                    </span>
                    {displayData.total_size > 0 && (
                        <span className="hex-view__size-label">
                            {displayData.total_size.toLocaleString()} bytes total
                        </span>
                    )}
                </div>
            )}

            {/* Patch controls — standalone mode only */}
            {isStandalone && (
                <div className="hex-patch__bar">
                    <input
                        className="hex-patch__input"
                        type="text"
                        placeholder="Offset (hex)"
                        value={patchOffset}
                        onChange={(e) => setPatchOffset(e.target.value)}
                        spellCheck={false}
                        style={{ width: 120 }}
                    />
                    <input
                        className="hex-patch__input"
                        type="text"
                        placeholder="Bytes (e.g. 90 90 90)"
                        value={patchBytes}
                        onChange={(e) => { setPatchBytes(e.target.value); setQuickPatch(""); }}
                        spellCheck={false}
                        style={{ width: 175 }}
                    />
                    <select
                        className="hex-patch__select"
                        value={quickPatch}
                        onChange={(e) => { setQuickPatch(e.target.value); setPatchBytes(""); }}
                    >
                        <option value="">Quick Patch…</option>
                        {QUICK_PATCHES.map((p) => (
                            <option key={p.label} value={p.label}>{p.label}</option>
                        ))}
                    </select>
                    <button className="hex-patch__btn hex-patch__btn--apply" onClick={handleApplyPatch}>
                        Apply
                    </button>
                    <button className="hex-patch__btn hex-patch__btn--save" onClick={handleSaveAs}>
                        💾 Save As…
                    </button>
                    {patchStatus && <span className="hex-patch__status">{patchStatus}</span>}
                </div>
            )}

            {/* Column header */}
            <div className="hex-view__header">
                <span className="hex-view__header-offset">Offset</span>
                <span className="hex-view__header-hex">
                    {Array.from({ length: 16 }, (_, i) =>
                        <span key={i} className="hex-view__col-header">
                            {i.toString(16).toUpperCase().padStart(2, "0")}
                        </span>
                    )}
                </span>
                <span className="hex-view__header-ascii">ASCII</span>
            </div>

            {/* Hex rows */}
            <div className="hex-view__body">
                {displayData.rows.map((row: HexRow) => {
                    const isHighlighted = highlightAddress && row.offset === highlightAddress;
                    return (
                        <div
                            key={row.offset}
                            className={`hex-row ${isHighlighted ? "hex-row--highlight" : ""}`}
                            onClick={() => isStandalone && setPatchOffset(row.offset)}
                            title={isStandalone ? "Click to set patch offset" : undefined}
                        >
                            <span className="hex-row__offset">{row.offset}</span>
                            <span className="hex-row__bytes">
                                {row.hex.slice(0, 8).map((byte, i) => (
                                    <span key={i} className={`hex-byte ${byte === "00" ? "hex-byte--zero" : ""}`}>
                                        {byte}
                                    </span>
                                ))}
                                <span className="hex-byte hex-byte--gap"> </span>
                                {row.hex.slice(8).map((byte, i) => (
                                    <span key={i + 8} className={`hex-byte ${byte === "00" ? "hex-byte--zero" : ""}`}>
                                        {byte}
                                    </span>
                                ))}
                                {Array.from({ length: 16 - row.hex.length }, (_, i) => (
                                    <span key={`pad-${i}`} className="hex-byte hex-byte--pad">  </span>
                                ))}
                            </span>
                            <span className="hex-row__ascii">{row.ascii}</span>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}

