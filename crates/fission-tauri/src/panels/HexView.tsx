import type { HexRow, HexViewData } from "../types";

interface HexViewProps {
    data: HexViewData | null;
    highlightAddress?: string | null;
}

export default function HexView({ data, highlightAddress }: HexViewProps) {
    if (!data || data.rows.length === 0) {
        return (
            <div className="hex-view hex-view--empty">
                <div className="hex-view__placeholder">No hex data loaded</div>
            </div>
        );
    }

    return (
        <div className="hex-view">
            <div className="hex-view__header">
                <span className="hex-view__header-offset">Offset</span>
                <span className="hex-view__header-hex">
                    {Array.from({ length: 16 }, (_, i) =>
                        <span key={i} className="hex-view__col-header">{i.toString(16).toUpperCase().padStart(2, '0')}</span>
                    )}
                </span>
                <span className="hex-view__header-ascii">ASCII</span>
            </div>
            <div className="hex-view__body">
                {data.rows.map((row: HexRow) => {
                    const isHighlighted = highlightAddress && row.offset === highlightAddress;
                    return (
                        <div
                            key={row.offset}
                            className={`hex-row ${isHighlighted ? "hex-row--highlight" : ""}`}
                        >
                            <span className="hex-row__offset">{row.offset}</span>
                            <span className="hex-row__bytes">
                                {row.hex.map((byte, i) => (
                                    <span
                                        key={i}
                                        className={`hex-byte ${byte === "00" ? "hex-byte--zero" : ""}`}
                                    >
                                        {byte}
                                    </span>
                                ))}
                                {/* Pad if less than 16 bytes */}
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
