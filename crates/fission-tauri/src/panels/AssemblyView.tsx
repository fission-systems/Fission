import type { AsmInstructionDto } from "../types";

interface Props {
    instructions: AsmInstructionDto[] | null;
}

export default function AssemblyView({ instructions }: Props) {
    if (instructions === null) {
        return (
            <div className="loading">
                <div className="spinner" />
                Disassembling...
            </div>
        );
    }

    if (instructions.length === 0) {
        return <div className="empty-state">No instructions</div>;
    }

    return (
        <div
            style={{
                height: "100%",
                overflow: "auto",
                fontFamily: "var(--font-mono)",
                fontSize: "var(--font-size-sm)",
                padding: "8px 0",
                lineHeight: "1.6",
            }}
        >
            {instructions.map((insn, i) => (
                <div
                    key={i}
                    style={{
                        display: "flex",
                        padding: "1px 16px",
                        gap: "16px",
                        cursor: "pointer",
                    }}
                    className="function-list__item"
                >
                    <span style={{ color: "var(--ctp-overlay1)", minWidth: 100, flexShrink: 0 }}>
                        {insn.address}
                    </span>
                    <span style={{ color: "var(--ctp-surface2)", minWidth: 120, flexShrink: 0, fontSize: 11 }}>
                        {insn.bytes}
                    </span>
                    <span style={{ color: "var(--ctp-mauve)", minWidth: 60, flexShrink: 0, fontWeight: 600 }}>
                        {insn.mnemonic}
                    </span>
                    <span style={{ color: "var(--ctp-text)" }}>
                        {insn.operands}
                    </span>
                </div>
            ))}
        </div>
    );
}
