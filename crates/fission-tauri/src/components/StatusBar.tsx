import type { BinaryInfo } from "../types";

interface StatusBarProps {
    binaryInfo: BinaryInfo | null;
    functionCount: number;
}

export default function StatusBar({ binaryInfo, functionCount }: StatusBarProps) {
    return (
        <div className="status-bar">
            {binaryInfo ? (
                <>
                    <span className="status-bar__item">📄 {binaryInfo.name}</span>
                    <span className="status-bar__item">{binaryInfo.format} · {binaryInfo.arch}</span>
                    <span className="status-bar__item">Entry: {binaryInfo.entry_point}</span>
                    <span className="status-bar__item">Functions: {functionCount}</span>
                    <span className="status-bar__item">Sections: {binaryInfo.section_count}</span>
                </>
            ) : (
                <span className="status-bar__item">No binary loaded</span>
            )}
        </div>
    );
}
