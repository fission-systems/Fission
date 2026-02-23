import { useMemo } from "react";
import type { XrefDto } from "../../types";

interface XrefsPanelProps {
    xrefs: XrefDto[];
    address: string | null;
    onXrefClick?: (address: string) => void;
}

export default function XrefsPanel({ xrefs, address, onXrefClick }: XrefsPanelProps) {
    // Count how many xrefs originate from the same function (incoming)
    // or point to the same target (outgoing) to show a badge.
    const funcCounts = useMemo(() => {
        const map = new Map<string, number>();
        for (const x of xrefs) {
            const key = x.from_function ?? x.from_address;
            map.set(key, (map.get(key) ?? 0) + 1);
        }
        return map;
    }, [xrefs]);

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
                        const funcKey = xref.from_function ?? xref.from_address;
                        const count = funcCounts.get(funcKey) ?? 1;
                        return (
                            <tr
                                key={i}
                                className="data-table__row"
                                onClick={() => onXrefClick?.(isIncoming ? xref.from_address : xref.to_address)}
                            >
                                <td className={isIncoming ? "xref-incoming" : "xref-outgoing"}>
                                    {isIncoming ? "← IN" : "→ OUT"}
                                </td>
                                <td className="data-table__addr">
                                    {isIncoming ? xref.from_address : xref.to_address}
                                </td>
                                <td>{xref.xref_type}</td>
                                <td>{xref.from_function ?? "—"}</td>
                                <td className="xrefs-panel__fn-count">
                                    {count > 1 ? (
                                        <span className="xrefs-panel__badge" title={`${count} xrefs from this function`}>
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
