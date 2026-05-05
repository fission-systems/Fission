import { useMemo } from "react";
import type { XrefDto } from "../../types";

interface XrefsPanelProps {
    xrefs: XrefDto[];
    address: string | null;
    onXrefClick?: (address: string) => void;
}

export default function XrefsPanel({ xrefs, address, onXrefClick }: XrefsPanelProps) {
    const funcCounts = useMemo(() => {
        const map = new Map<string, number>();
        if (!address) {
            return map;
        }
        for (const x of xrefs) {
            const incoming = x.to_address === address;
            const key = incoming
                ? (x.from_function ?? x.from_address)
                : (x.to_function ?? x.to_address);
            map.set(key, (map.get(key) ?? 0) + 1);
        }
        return map;
    }, [xrefs, address]);

    if (!address) {
        return (
            <div className="xrefs-panel xrefs-panel--empty">
                Select a function to view cross-references
            </div>
        );
    }

    if (xrefs.length === 0) {
        return (
            <div className="xrefs-panel xrefs-panel--empty">
                No cross-references found for {address}
            </div>
        );
    }

    return (
        <div className="xrefs-panel">
            <div className="xrefs-panel__count-bar">
                {xrefs.length} cross-reference{xrefs.length !== 1 ? "s" : ""}{" "}
                from {funcCounts.size} function{funcCounts.size !== 1 ? "s" : ""}
            </div>
            <table className="data-table">
                <thead>
                    <tr>
                        <th>Dir</th>
                        <th>Address</th>
                        <th>Type</th>
                        <th>Function</th>
                        <th title="Xrefs from same function">#</th>
                    </tr>
                </thead>
                <tbody>
                    {xrefs.map((xref, i) => {
                        const isIncoming = xref.to_address === address;
                        const funcKey = isIncoming
                            ? (xref.from_function ?? xref.from_address)
                            : (xref.to_function ?? xref.to_address);
                        const count = funcCounts.get(funcKey) ?? 1;
                        const funcLabel = isIncoming
                            ? (xref.from_function ?? "—")
                            : (xref.to_function ?? "—");
                        const meta =
                            xref.sleigh_kind != null || xref.operand_index != null
                                ? `${xref.sleigh_kind ?? ""}${xref.sleigh_kind != null && xref.operand_index != null ? " " : ""}${xref.operand_index != null ? `op:${xref.operand_index}` : ""}`.trim()
                                : null;
                        return (
                            <tr
                                key={i}
                                className="data-table__row"
                                onClick={() =>
                                    onXrefClick?.(isIncoming ? xref.from_address : xref.to_address)
                                }
                                title={meta ?? undefined}
                            >
                                <td className={isIncoming ? "xref-incoming" : "xref-outgoing"}>
                                    {isIncoming ? "← IN" : "→ OUT"}
                                </td>
                                <td className="data-table__addr">
                                    {isIncoming ? xref.from_address : xref.to_address}
                                </td>
                                <td>{xref.xref_type}</td>
                                <td>{funcLabel}</td>
                                <td className="xrefs-panel__fn-count">
                                    {count > 1 ? (
                                        <span
                                            className="xrefs-panel__badge"
                                            title={`${count} xrefs grouped`}
                                        >
                                            ×{count}
                                        </span>
                                    ) : null}
                                </td>
                            </tr>
                        );
                    })}
                </tbody>
            </table>
        </div>
    );
}
