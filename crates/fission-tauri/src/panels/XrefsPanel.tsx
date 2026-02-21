import type { XrefDto } from "../types";

interface XrefsPanelProps {
    xrefs: XrefDto[];
    address: string | null;
    onXrefClick?: (address: string) => void;
}

export default function XrefsPanel({ xrefs, address, onXrefClick }: XrefsPanelProps) {
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
            <table className="data-table">
                <thead>
                    <tr>
                        <th>Direction</th>
                        <th>Address</th>
                        <th>Type</th>
                        <th>Function</th>
                    </tr>
                </thead>
                <tbody>
                    {xrefs.map((xref, i) => {
                        const isIncoming = xref.to_address === address;
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
                                <td>{xref.from_function || "—"}</td>
                            </tr>
                        );
                    })}
                </tbody>
            </table>
        </div>
    );
}
