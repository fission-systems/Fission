from __future__ import annotations

import time
from pathlib import Path
from typing import Any


def write_markdown_report(report: dict[str, Any], output_path: Path) -> None:
    lines = [
        "# GRAND-FINALE Report",
        "",
        f"- Generated: {time.strftime('%Y-%m-%d %H:%M:%S')}",
        f"- Binaries: {len(report['binaries'])}",
        "",
        "## Global Summary",
        "",
        f"- Shared successful functions: {report['global']['shared_success_count']}",
        f"- Fission success count: {report['global']['fission_success_count']}",
        f"- Ghidra success count: {report['global']['ghidra_success_count']}",
        f"- Goto reduction vs Ghidra: {report['global']['control_flow']['goto_reduction_pct']:.2f}%",
        f"- Fission switches / Ghidra switches: {report['global']['control_flow']['fission_switches']} / {report['global']['control_flow']['ghidra_switches']}",
        f"- Fission for-loops / Ghidra for-loops: {report['global']['control_flow']['fission_for_loops']} / {report['global']['control_flow']['ghidra_for_loops']}",
        f"- Fission do-while / Ghidra do-while: {report['global']['control_flow']['fission_do_while']} / {report['global']['control_flow']['ghidra_do_while']}",
        f"- Failure classes (Fission): {report['global']['failure_class_counts']['fission']}",
        f"- Failure classes (Ghidra): {report['global']['failure_class_counts']['ghidra']}",
        f"- Type preservation hits (Fission): {report['global']['type_preservation_counts']['fission']}",
        f"- Raw pointer / assembly fallbacks (Fission): {report['global']['fallback_counts']['fission']}",
        f"- Cast chains (Fission/Ghidra): {report['global']['cast_chain_counts']['fission']} / {report['global']['cast_chain_counts']['ghidra']}",
        f"- MLIL preview success / residue / cast density: {report['global']['mlil_preview_success']} / {report['global']['mlil_preview_residue']} / {report['global']['mlil_preview_cast_density']}",
        "",
        "## Preview vs Legacy",
        "",
        "| Metric | Preview | Legacy/Fission |",
        "| --- | ---: | ---: |",
        f"| Engine used count | {report['global']['preview_engine_used_count']} | {report['global']['fission_success_count']} |",
        f"| Fallback count | {report['global']['preview_fallback_count']} | n/a |",
        f"| Goto count | {report['global']['preview_goto_count']} | {report['global']['control_flow']['fission_gotos']} |",
        f"| Temp surface count | {report['global']['preview_temp_surface_count']} | {sum(report['global']['residue_rankings']['residue_families'].get(k, 0) for k in ('uVar','iVar','xVar','bVar','uStack','xStack','axStack'))} |",
        f"| Cast density | {report['global']['mlil_preview_cast_density']} | {report['global']['cast_chain_counts']['fission']} |",
        "",
        "## Residue Intel",
        "",
    ]

    for title, payload in [
        ("Single-Assign Temps", report["global"]["residue_rankings"]["single_assign_temps"]),
        ("Residue Names", report["global"]["residue_rankings"]["residue_names"]),
        ("Residue Families", report["global"]["residue_rankings"]["residue_families"]),
    ]:
        lines.append(f"### {title}")
        if not payload:
            lines.append("- none")
        else:
            for name, count in payload.items():
                lines.append(f"- `{name}`: {count}")
        lines.append("")

    lines.append("### Top Offenders")
    if not report["global"]["top_residue_offenders"]:
        lines.append("- none")
    else:
        for offender in report["global"]["top_residue_offenders"]:
            lines.append(
                f"- `{offender['binary']}` `{offender['address']}` `{offender['name']}`: "
                f"score={offender['residue_score']}, raw_pointer_fallback={offender['raw_pointer_fallback']}, "
                f"single_assign_temps={offender['single_assign_temp_total']}, "
                f"top_names={offender['top_residue_names']}"
            )
    lines.append("")

    lines.append("## Per-Binary Summary")
    lines.append("")
    for binary in report["binaries"]:
        lines.append(f"### {binary['binary']}")
        lines.append(
            f"- Shared success: {binary['shared_success_count']} / {binary['function_count']} | "
            f"Goto reduction: {binary['control_flow']['goto_reduction_pct']:.2f}%"
        )
        lines.append(
            f"- Fission/Ghidra success: {binary['fission_success_count']} / {binary['ghidra_success_count']}"
        )
        lines.append(
            f"- Struct pointer hits: Fission {sum(binary['type_promotion']['fission_hits'].values())}, "
            f"Ghidra {sum(binary['type_promotion']['ghidra_hits'].values())}"
        )
        lines.append(
            f"- Type preservation: Fission {binary['type_preservation_counts']['fission']}, "
            f"Ghidra {binary['type_preservation_counts']['ghidra']}"
        )
        lines.append(
            f"- Failure classes: Fission {binary['failure_class_counts']['fission']} | "
            f"Ghidra {binary['failure_class_counts']['ghidra']}"
        )
        lines.append(
            f"- Cast chains: Fission {binary['cast_chain_counts']['fission']} | "
            f"Ghidra {binary['cast_chain_counts']['ghidra']}"
        )
        lines.append(
            f"- MLIL preview success / residue / cast density: "
            f"{binary.get('mlil_preview_success', 0)} / "
            f"{binary.get('mlil_preview_residue', 0)} / "
            f"{binary.get('mlil_preview_cast_density', 0)}"
        )
        lines.append(
            f"- Preview engine used / fallback / goto / temp surface: "
            f"{binary.get('preview_engine_used_count', 0)} / "
            f"{binary.get('preview_fallback_count', 0)} / "
            f"{binary.get('preview_goto_count', 0)} / "
            f"{binary.get('preview_temp_surface_count', 0)}"
        )
        if binary["top_residue_offenders"]:
            lines.append("- Top residue offenders:")
            for offender in binary["top_residue_offenders"]:
                lines.append(
                    f"  - `{offender['address']}` `{offender['name']}`: "
                    f"score={offender['residue_score']}, raw_pointer_fallback={offender['raw_pointer_fallback']}, "
                    f"single_assign_temps={offender['single_assign_temp_total']}, "
                    f"top_names={offender['top_residue_names']}"
                )
        lines.append("")

    output_path.write_text("\n".join(lines))
