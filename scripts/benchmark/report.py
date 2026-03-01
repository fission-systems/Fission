#!/usr/bin/env python3
"""
report.py — HTML benchmark report generator for v4.

Generates a self-contained HTML report with 5 tabs:
  1. Overview — aggregate metrics matrix
  2. By Category — category-level breakdown
  3. All Functions — sortable table
  4. Low Similarity — functions below threshold
  5. Side-by-Side — code comparison viewer

Usage:
    python3 report.py --results results/results.json --summary results/summary.json -o report.html
"""

from __future__ import annotations

import argparse
import html
import json
import sys
from pathlib import Path
from typing import Any


# ===========================================================================
# Color helpers
# ===========================================================================

def _sim_color(sim: float | None) -> str:
    if sim is None:
        return "#999"
    if sim >= 90:
        return "#2ecc71"
    if sim >= 70:
        return "#f39c12"
    if sim >= 50:
        return "#e67e22"
    return "#e74c3c"


def _sim_bg(sim: float | None) -> str:
    if sim is None:
        return "#f0f0f0"
    if sim >= 90:
        return "#d5f5e3"
    if sim >= 70:
        return "#fdebd0"
    if sim >= 50:
        return "#fadbd8"
    return "#f5b7b1"


def _chk_color(ratio: float) -> str:
    if ratio >= 0.9:
        return "#2ecc71"
    if ratio >= 0.6:
        return "#f39c12"
    return "#e74c3c"


# ===========================================================================
# HTML generation
# ===========================================================================

def _esc(text: str) -> str:
    return html.escape(str(text))


def _build_css() -> str:
    return """
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
         font-size: 14px; color: #333; background: #fafafa; }
  .container { max-width: 1400px; margin: 0 auto; padding: 20px; }
  h1 { font-size: 1.8em; margin-bottom: 10px; }
  h2 { font-size: 1.3em; margin: 20px 0 10px; color: #2c3e50; }
  .meta { color: #777; font-size: 0.85em; margin-bottom: 15px; }

  /* Tabs */
  .tabs { display: flex; border-bottom: 2px solid #ddd; margin-bottom: 15px; }
  .tab { padding: 10px 20px; cursor: pointer; border-bottom: 3px solid transparent;
         color: #777; font-weight: 500; }
  .tab:hover { color: #2c3e50; }
  .tab.active { color: #2c3e50; border-color: #3498db; }
  .tab-content { display: none; }
  .tab-content.active { display: block; }

  /* Tables */
  table { width: 100%; border-collapse: collapse; margin-bottom: 15px; }
  th { background: #34495e; color: #fff; text-align: left; padding: 8px 12px;
       font-weight: 500; font-size: 0.85em; cursor: pointer; user-select: none; }
  th:hover { background: #2c3e50; }
  td { padding: 8px 12px; border-bottom: 1px solid #eee; }
  tr:hover td { background: #f5f6fa; }
  .mono { font-family: "SFMono-Regular", Consolas, monospace; font-size: 0.85em; }

  /* Badges */
  .badge { display: inline-block; padding: 2px 8px; border-radius: 10px;
           font-size: 0.8em; font-weight: 600; color: #fff; }
  .sim-badge { min-width: 50px; text-align: center; }

  /* Cards */
  .card { background: #fff; border-radius: 8px; padding: 16px; margin-bottom: 12px;
          box-shadow: 0 1px 3px rgba(0,0,0,0.1); }
  .card-title { font-weight: 600; margin-bottom: 8px; }

  /* Stats grid */
  .stats { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
           gap: 12px; margin-bottom: 20px; }
  .stat-card { background: #fff; border-radius: 8px; padding: 16px; text-align: center;
               box-shadow: 0 1px 3px rgba(0,0,0,0.1); }
  .stat-value { font-size: 2em; font-weight: 700; }
  .stat-label { color: #777; font-size: 0.85em; }

  /* Code blocks */
  .code-container { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin-top: 10px; }
  .code-block { background: #1e1e1e; color: #d4d4d4; padding: 12px; border-radius: 6px;
                overflow-x: auto; font-family: "SFMono-Regular", Consolas, monospace;
                font-size: 0.8em; white-space: pre; max-height: 500px; overflow-y: auto; }
  .code-label { font-weight: 600; margin-bottom: 5px; font-size: 0.9em; }
  .expand-btn { color: #3498db; cursor: pointer; text-decoration: underline; font-size: 0.85em; }

  /* Distribution bar */
  .dist-bar { display: flex; height: 24px; border-radius: 4px; overflow: hidden; margin: 5px 0; }
  .dist-seg { display: flex; align-items: center; justify-content: center;
              font-size: 0.75em; font-weight: 600; color: #fff; min-width: 1px; }
</style>
"""


def _build_js() -> str:
    return """
<script>
function switchTab(tabId) {
    document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
    document.querySelectorAll('.tab-content').forEach(c => c.classList.remove('active'));
    document.querySelector('[data-tab="'+tabId+'"]').classList.add('active');
    document.getElementById(tabId).classList.add('active');
}

function sortTable(tableId, col) {
    const table = document.getElementById(tableId);
    const tbody = table.querySelector('tbody');
    const rows = Array.from(tbody.rows);
    const th = table.querySelectorAll('th')[col];
    const asc = th.dataset.sort !== 'asc';
    th.dataset.sort = asc ? 'asc' : 'desc';

    rows.sort((a, b) => {
        let va = a.cells[col].dataset.val || a.cells[col].textContent.trim();
        let vb = b.cells[col].dataset.val || b.cells[col].textContent.trim();
        let na = parseFloat(va), nb = parseFloat(vb);
        if (!isNaN(na) && !isNaN(nb)) return asc ? na - nb : nb - na;
        return asc ? va.localeCompare(vb) : vb.localeCompare(va);
    });
    rows.forEach(r => tbody.appendChild(r));
}

function toggleCode(id) {
    const el = document.getElementById(id);
    el.style.display = el.style.display === 'none' ? 'block' : 'none';
}
</script>
"""


def _tab_overview(summary: dict) -> str:
    sim = summary.get("similarity", {})
    chk = summary.get("checklist", {})
    dist = summary.get("similarity_distribution", {})

    parts = ['<div class="stats">']

    # Stat cards
    stats = [
        (f"{summary.get('total_functions', 0)}", "Total Functions"),
        (f"{summary.get('compared_functions', 0)}", "Compared"),
        (f"{summary.get('fission_errors', 0)}", "Errors"),
        (f"{sim.get('avg', 'N/A')}%", "Avg Similarity"),
        (f"{sim.get('median', 'N/A')}%", "Median Similarity"),
        (f"{chk.get('avg_ratio', 0):.0%}" if chk else "N/A", "Checklist Score"),
    ]
    for val, label in stats:
        parts.append(f'<div class="stat-card"><div class="stat-value">{_esc(str(val))}</div>'
                     f'<div class="stat-label">{_esc(label)}</div></div>')
    parts.append("</div>")

    # Distribution bar
    if dist:
        total = sum(dist.values()) or 1
        colors = {">=90%": "#2ecc71", "70-90%": "#f39c12", "50-70%": "#e67e22", "<50%": "#e74c3c"}
        parts.append('<h2>Similarity Distribution</h2><div class="dist-bar">')
        for bucket, color in colors.items():
            count = dist.get(bucket, 0)
            pct = count / total * 100
            parts.append(f'<div class="dist-seg" style="width:{pct}%;background:{color}">'
                         f'{count}</div>')
        parts.append("</div>")

    # By category table
    by_cat = summary.get("by_category", {})
    if by_cat:
        parts.append('<h2>By Category</h2><table><thead><tr>'
                     '<th>Category</th><th>Count</th><th>Avg Similarity</th>'
                     '<th>Min</th><th>Max</th></tr></thead><tbody>')
        for cat, info in sorted(by_cat.items()):
            avg = info["avg_similarity"]
            parts.append(f'<tr><td>{_esc(cat)}</td><td>{info["count"]}</td>'
                         f'<td><span class="badge sim-badge" style="background:{_sim_color(avg)}">'
                         f'{avg:.1f}%</span></td>'
                         f'<td>{info["min"]:.0f}%</td><td>{info["max"]:.0f}%</td></tr>')
        parts.append("</tbody></table>")

    # By format & opt level
    for group_key, group_name in [("by_format", "By Format"), ("by_opt_level", "By Optimization Level")]:
        grp = summary.get(group_key, {})
        if grp:
            parts.append(f'<h2>{group_name}</h2><table><thead><tr>'
                         f'<th>{group_name.split()[-1]}</th><th>Count</th><th>Avg Similarity</th>'
                         f'</tr></thead><tbody>')
            for k, info in sorted(grp.items()):
                avg = info["avg_similarity"]
                parts.append(f'<tr><td>{_esc(k)}</td><td>{info["count"]}</td>'
                             f'<td><span class="badge sim-badge" style="background:{_sim_color(avg)}">'
                             f'{avg:.1f}%</span></td></tr>')
            parts.append("</tbody></table>")

    return "\n".join(parts)


def _tab_all_functions(results: list[dict]) -> str:
    parts = ['<table id="tbl-all"><thead><tr>']
    headers = ["Function", "Binary", "Category", "Opt", "Format", "Similarity", "Checklist", "Lines(F)", "Errors"]
    for i, h in enumerate(headers):
        parts.append(f'<th onclick="sortTable(\'tbl-all\',{i})">{h}</th>')
    parts.append("</tr></thead><tbody>")

    for r in sorted(results, key=lambda x: x.get("similarity") or 0):
        sim = r.get("similarity")
        sim_str = f"{sim:.1f}%" if sim is not None else "N/A"
        chk = r.get("fission_checklist", {})
        chk_str = f"{chk['satisfied']}/{chk['total']}" if chk else "-"
        lines = r.get("fission_metrics", {}).get("lines", 0)
        err = "Yes" if r.get("fission_has_error") else "-"

        binary_short = Path(r.get("binary", "")).name

        parts.append(f'<tr><td class="mono">{_esc(r.get("function", ""))}</td>'
                     f'<td class="mono" title="{_esc(r.get("binary", ""))}">{_esc(binary_short)}</td>'
                     f'<td>{_esc(r.get("category", ""))}</td>'
                     f'<td>{_esc(r.get("opt_level", ""))}</td>'
                     f'<td>{_esc(r.get("format", ""))}</td>'
                     f'<td data-val="{sim if sim is not None else -1}">'
                     f'<span class="badge sim-badge" style="background:{_sim_color(sim)}">'
                     f'{sim_str}</span></td>'
                     f'<td>{chk_str}</td>'
                     f'<td>{lines}</td>'
                     f'<td>{err}</td></tr>')

    parts.append("</tbody></table>")
    return "\n".join(parts)


def _tab_low_sim(results: list[dict], threshold: float = 70.0) -> str:
    low = [r for r in results if r.get("similarity") is not None and r["similarity"] < threshold]
    low.sort(key=lambda r: r["similarity"])

    if not low:
        return f'<p>No functions below {threshold}% similarity.</p>'

    parts = [f'<p>{len(low)} functions below {threshold}% similarity:</p>']
    for r in low:
        fname = r.get("function", r.get("address", ""))
        sim = r["similarity"]
        parts.append(f'<div class="card">')
        parts.append(f'<div class="card-title">'
                     f'<span class="badge sim-badge" style="background:{_sim_color(sim)}">'
                     f'{sim:.1f}%</span> '
                     f'{_esc(fname)} — {_esc(Path(r.get("binary", "")).name)} '
                     f'[{_esc(r.get("category", ""))}]</div>')

        # Checklist details
        chk = r.get("fission_checklist", {})
        if chk:
            patterns = chk.get("patterns", {})
            missing = [p for p, hit in patterns.items() if not hit]
            if missing:
                parts.append(f'<div style="color:#e74c3c;font-size:0.85em">Missing patterns: '
                             f'{", ".join(_esc(p) for p in missing)}</div>')

        parts.append("</div>")

    return "\n".join(parts)


def _tab_side_by_side(results: list[dict]) -> str:
    """Code comparison viewer (first 20 functions)."""
    shown = [r for r in sorted(results, key=lambda x: x.get("similarity") or 0) if r.get("ghidra_code")]
    parts = []

    for idx, r in enumerate(shown[:20]):
        fid = f"code-{idx}"
        fname = r.get("function", r.get("address", ""))
        sim = r.get("similarity")
        sim_str = f"{sim:.1f}%" if sim is not None else "N/A"

        parts.append(f'<div class="card">')
        parts.append(f'<div class="card-title">'
                     f'<span class="badge sim-badge" style="background:{_sim_color(sim)}">'
                     f'{sim_str}</span> '
                     f'{_esc(fname)} — {_esc(Path(r.get("binary", "")).name)} '
                     f'<span class="expand-btn" onclick="toggleCode(\'{fid}\')">[toggle code]</span>'
                     f'</div>')

        display = "block" if idx < 3 else "none"
        parts.append(f'<div id="{fid}" style="display:{display}">')
        parts.append('<div class="code-container">')

        ghidra_code = r.get("ghidra_code", "")
        fission_code = r.get("fission_code", "")
        parts.append(f'<div><div class="code-label">Ghidra</div>'
                     f'<div class="code-block">{_esc(ghidra_code)}</div></div>')
        parts.append(f'<div><div class="code-label">Fission</div>'
                     f'<div class="code-block">{_esc(fission_code)}</div></div>')

        parts.append("</div></div></div>")

    return "\n".join(parts)


def _tab_by_category(summary: dict, results: list[dict]) -> str:
    by_cat = summary.get("by_category", {})
    if not by_cat:
        return "<p>No category data available.</p>"

    parts = []
    for cat in sorted(by_cat.keys()):
        info = by_cat[cat]
        avg = info["avg_similarity"]
        cat_results = [r for r in results if r.get("category") == cat]

        parts.append(f'<div class="card"><div class="card-title">'
                     f'<span class="badge sim-badge" style="background:{_sim_color(avg)}">'
                     f'{avg:.1f}%</span> {_esc(cat)} ({info["count"]} functions)</div>')

        parts.append('<table><thead><tr><th>Function</th><th>Opt</th><th>Similarity</th>'
                     '<th>Checklist</th></tr></thead><tbody>')
        for r in sorted(cat_results, key=lambda x: x.get("similarity") or 0):
            sim = r.get("similarity")
            sim_str = f"{sim:.1f}%" if sim is not None else "N/A"
            chk = r.get("fission_checklist", {})
            chk_str = f"{chk['satisfied']}/{chk['total']}" if chk else "-"
            parts.append(f'<tr><td class="mono">{_esc(r.get("function", ""))}</td>'
                         f'<td>{_esc(r.get("opt_level", ""))}</td>'
                         f'<td><span class="badge sim-badge" style="background:{_sim_color(sim)}">'
                         f'{sim_str}</span></td><td>{chk_str}</td></tr>')
        parts.append("</tbody></table></div>")

    return "\n".join(parts)


def generate_html_report(
    results: list[dict],
    summary: dict,
    output_path: Path,
) -> None:
    """Generate self-contained HTML report."""
    env = summary.get("env", {})
    suite_name = summary.get("suite", "Benchmark")

    body_parts = [
        '<!DOCTYPE html><html lang="en"><head><meta charset="utf-8">',
        f'<title>Fission Benchmark — {_esc(suite_name)}</title>',
        _build_css(),
        '</head><body><div class="container">',
        f'<h1>Fission vs Ghidra — {_esc(suite_name)}</h1>',
        f'<div class="meta">{_esc(env.get("run_at", ""))} · '
        f'rev {_esc(env.get("fission_git_rev", "?"))} · '
        f'{_esc(env.get("platform", ""))}</div>',

        # Tabs
        '<div class="tabs">',
        '<div class="tab active" data-tab="tab-overview" onclick="switchTab(\'tab-overview\')">Overview</div>',
        '<div class="tab" data-tab="tab-category" onclick="switchTab(\'tab-category\')">By Category</div>',
        '<div class="tab" data-tab="tab-all" onclick="switchTab(\'tab-all\')">All Functions</div>',
        '<div class="tab" data-tab="tab-low" onclick="switchTab(\'tab-low\')">Low Similarity</div>',
        '<div class="tab" data-tab="tab-sidebyside" onclick="switchTab(\'tab-sidebyside\')">Side-by-Side</div>',
        '</div>',

        # Tab contents
        '<div id="tab-overview" class="tab-content active">',
        _tab_overview(summary),
        '</div>',

        '<div id="tab-category" class="tab-content">',
        _tab_by_category(summary, results),
        '</div>',

        '<div id="tab-all" class="tab-content">',
        _tab_all_functions(results),
        '</div>',

        '<div id="tab-low" class="tab-content">',
        _tab_low_sim(results),
        '</div>',

        '<div id="tab-sidebyside" class="tab-content">',
        _tab_side_by_side(results),
        '</div>',

        _build_js(),
        '</div></body></html>',
    ]

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text("\n".join(body_parts), encoding="utf-8")
    print(f"HTML report written to {output_path} ({output_path.stat().st_size / 1024:.1f} KB)")


# ===========================================================================
# CLI
# ===========================================================================

def main() -> int:
    parser = argparse.ArgumentParser(description="Generate HTML benchmark report")
    parser.add_argument("--results", required=True, help="Path to results.json")
    parser.add_argument("--summary", required=True, help="Path to summary.json")
    parser.add_argument("-o", "--output", default="report.html", help="Output HTML file")
    args = parser.parse_args()

    results_path = Path(args.results)
    summary_path = Path(args.summary)

    if not results_path.exists():
        print(f"Error: {results_path} not found")
        return 1
    if not summary_path.exists():
        print(f"Error: {summary_path} not found")
        return 1

    with open(results_path, "r") as f:
        results = json.load(f)
    with open(summary_path, "r") as f:
        summary = json.load(f)

    generate_html_report(results, summary, Path(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
