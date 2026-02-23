import { useState, useCallback, useRef, useEffect } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { AsmInstructionDto } from "../../types";

// Mnemonic color categories
const BRANCH_MNEMONICS = new Set(["jmp", "je", "jne", "jz", "jnz", "jg", "jge", "jl", "jle", "ja", "jae", "jb", "jbe", "jo", "jno", "js", "jns", "jp", "jnp", "jcxz", "jecxz", "jrcxz", "loop", "loope", "loopne"]);
const CALL_MNEMONICS = new Set(["call"]);
const RET_MNEMONICS = new Set(["ret", "retn", "retf", "iret", "iretd", "iretq"]);
const STACK_MNEMONICS = new Set(["push", "pop", "pusha", "popa", "pushf", "popf", "pushfd", "popfd", "pushfq", "popfq"]);
const NOP_MNEMONICS = new Set(["nop", "int3"]);

function getMnemonicClass(mnemonic: string): string {
    const m = mnemonic.toLowerCase();
    if (CALL_MNEMONICS.has(m)) return "asm-call";
    if (BRANCH_MNEMONICS.has(m)) return "asm-branch";
    if (RET_MNEMONICS.has(m)) return "asm-ret";
    if (STACK_MNEMONICS.has(m)) return "asm-stack";
    if (NOP_MNEMONICS.has(m)) return "asm-nop";
    if (m.startsWith("mov") || m.startsWith("lea") || m.startsWith("xchg")) return "asm-mov";
    if (m.startsWith("cmp") || m.startsWith("test")) return "asm-cmp";
    return "asm-default";
}

// Check if operand looks like a hex address
function isAddressLike(operand: string): boolean {
    return /^0x[0-9a-fA-F]{4,}$/.test(operand.trim());
}

interface AssemblyViewProps {
    instructions: AsmInstructionDto[] | null;
    onAddressClick?: (address: string) => void;
    onCommentEdit?: (address: string, currentComment: string) => void;
    onRename?: (address: string, currentName: string) => void;
    onToggleBookmark?: (address: string) => void;
    selectedAddress?: string | null;
    functionName?: string;
    /** Called when the user scrolls near the bottom to load more rows */
    onLoadMore?: () => void;
    /** Whether more rows are available to load */
    hasMore?: boolean;
    /** Whether a load-more fetch is in progress */
    loadingMore?: boolean;
}

const ROW_HEIGHT = 22; // px — fixed height for virtualizer estimate
const OVERSCAN = 20;   // extra rows rendered above/below viewport

export default function AssemblyView({
    instructions,
    onAddressClick,
    onCommentEdit,
    onRename,
    onToggleBookmark,
    selectedAddress,
    functionName,
    onLoadMore,
    hasMore = false,
    loadingMore = false,
}: AssemblyViewProps) {
    const [contextMenu, setContextMenu] = useState<{ x: number; y: number; address: string; comment: string } | null>(null);
    const scrollRef = useRef<HTMLDivElement>(null);

    const handleContextMenu = useCallback((e: React.MouseEvent, addr: string, comment: string) => {
        e.preventDefault();
        setContextMenu({ x: e.clientX, y: e.clientY, address: addr, comment: comment || "" });
    }, []);

    const closeContext = useCallback(() => setContextMenu(null), []);

    const rowVirtualizer = useVirtualizer({
        count: instructions?.length ?? 0,
        getScrollElement: () => scrollRef.current,
        estimateSize: () => ROW_HEIGHT,
        overscan: OVERSCAN,
    });

    // Trigger load-more when scrolled within 200px of the bottom
    useEffect(() => {
        const el = scrollRef.current;
        if (!el || !onLoadMore || !hasMore) return;
        const onScroll = () => {
            if (el.scrollTop + el.clientHeight >= el.scrollHeight - 200) {
                onLoadMore();
            }
        };
        el.addEventListener("scroll", onScroll, { passive: true });
        return () => el.removeEventListener("scroll", onScroll);
    }, [onLoadMore, hasMore]);

    if (!instructions) {
        return (
            <div className="asm-view asm-view--empty">
                <div className="asm-view__placeholder">Select a function to view assembly</div>
            </div>
        );
    }

    // Parse operands and make address-like tokens clickable
    const renderOperands = (operands: string) => {
        const parts = operands.split(/(0x[0-9a-fA-F]+)/g);
        return parts.map((part, i) => {
            if (isAddressLike(part)) {
                return (
                    <span
                        key={i}
                        className="asm-address-link"
                        onClick={() => onAddressClick?.(part)}
                        title={`Go to ${part}`}
                    >
                        {part}
                    </span>
                );
            }
            return <span key={i}>{part}</span>;
        });
    };

    const virtualItems = rowVirtualizer.getVirtualItems();
    const totalSize = rowVirtualizer.getTotalSize();
    const paddingTop = virtualItems.length > 0 ? virtualItems[0].start : 0;
    const paddingBottom = virtualItems.length > 0
        ? totalSize - virtualItems[virtualItems.length - 1].end
        : totalSize;

    return (
        <div className="asm-view" onClick={closeContext}>
            {/* Scrollable container — virtualizer scrolls here */}
            <div ref={scrollRef} className="asm-view__scroller">
                <table className="asm-table">
                    <thead>
                        <tr>
                            <th className="asm-col-addr">Address</th>
                            <th className="asm-col-bytes">Bytes</th>
                            <th className="asm-col-mnemonic">Mnemonic</th>
                            <th className="asm-col-operands">Operands</th>
                            <th className="asm-col-comment">Comment</th>
                        </tr>
                    </thead>
                    <tbody>
                        {/* Top spacer */}
                        {paddingTop > 0 && (
                            <tr><td style={{ height: paddingTop }} colSpan={5} /></tr>
                        )}

                        {virtualItems.map((vRow) => {
                            const insn = instructions[vRow.index];
                            return (
                                <tr
                                    key={vRow.key}
                                    data-index={vRow.index}
                                    ref={rowVirtualizer.measureElement}
                                    className={`asm-row ${selectedAddress === insn.address ? "asm-row--selected" : ""}`}
                                    onContextMenu={(e) => handleContextMenu(e, insn.address, insn.comment || "")}
                                >
                                    <td className="asm-addr">{insn.address}</td>
                                    <td className="asm-bytes">{insn.bytes}</td>
                                    <td className={`asm-mnemonic ${getMnemonicClass(insn.mnemonic)}`}>
                                        {insn.mnemonic}
                                    </td>
                                    <td className="asm-operands">{renderOperands(insn.operands)}</td>
                                    <td
                                        className="asm-comment"
                                        onDoubleClick={() => onCommentEdit?.(insn.address, insn.comment || "")}
                                        title="Double-click to edit comment"
                                    >
                                        {insn.comment && <span className="asm-comment-text">; {insn.comment}</span>}
                                    </td>
                                </tr>
                            );
                        })}

                        {/* Bottom spacer */}
                        {paddingBottom > 0 && (
                            <tr><td style={{ height: paddingBottom }} colSpan={5} /></tr>
                        )}

                        {/* Load-more indicator */}
                        {hasMore && (
                            <tr className="asm-load-more-row">
                                <td colSpan={5} className="asm-load-more">
                                    {loadingMore ? "Loading…" : "Scroll down to load more"}
                                </td>
                            </tr>
                        )}
                    </tbody>
                </table>
            </div>

            {contextMenu && (
                <div
                    className="context-menu"
                    style={{ left: contextMenu.x, top: contextMenu.y }}
                    onClick={(e) => e.stopPropagation()}
                >
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onCommentEdit?.(contextMenu.address, contextMenu.comment);
                            closeContext();
                        }}
                    >
                        ✏️ Add/Edit Comment
                    </div>
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onRename?.(contextMenu.address, functionName || "");
                            closeContext();
                        }}
                    >
                        🏷️ Rename Label
                    </div>
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onToggleBookmark?.(contextMenu.address);
                            closeContext();
                        }}
                    >
                        📌 Add Bookmark
                    </div>
                    <div className="context-menu__separator" />
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            onAddressClick?.(contextMenu.address);
                            closeContext();
                        }}
                    >
                        🔍 Go to Address
                    </div>
                    <div
                        className="context-menu__item"
                        onClick={() => {
                            navigator.clipboard.writeText(contextMenu.address).catch(() => {});
                            closeContext();
                        }}
                    >
                        📋 Copy Address
                    </div>
                </div>
            )}
        </div>
    );
}
