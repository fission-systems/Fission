//! CFG Viewer — Sugiyama layered-graph layout with SVG rendering.
//!
//! Layout pipeline:
//!   1. BFS layer assignment (back edges ignored for DAG layer)
//!   2. Barycenter crossing reduction (2 passes)
//!   3. Coordinate assignment (fixed NODE_W, height ∝ op_count)
//!   4. Edge routing (orthogonal cubic bezier for forward; arc for back)

use crate::engine::{CfgEdgeKind, CfgGraphData, CfgNodeData};
use crate::state::use_app_state;
use dioxus::prelude::*;

// ── Layout constants ─────────────────────────────────────────────────────────
const NODE_W: f64 = 160.0;
const LAYER_GAP: f64 = 80.0;
const NODE_GAP: f64 = 36.0;
const SVG_PAD: f64 = 32.0;
const NODE_H_BASE: f64 = 44.0;
const NODE_H_PER_OP: f64 = 4.0;
const NODE_H_MAX: f64 = 80.0;
const BACK_OFFSET: f64 = 32.0;

fn node_h(n: &CfgNodeData) -> f64 {
    (NODE_H_BASE + n.op_count.min(9) as f64 * NODE_H_PER_OP).min(NODE_H_MAX)
}

// ── Layout ───────────────────────────────────────────────────────────────────
#[derive(Clone)]
struct LNode {
    idx: usize,
    layer: usize,
    x: f64,
    y: f64,
    h: f64,
}

fn compute_layout(cfg: &CfgGraphData) -> Vec<LNode> {
    let n = cfg.nodes.len();
    if n == 0 {
        return vec![];
    }

    let back_set: std::collections::HashSet<(usize, usize)> = cfg
        .edges
        .iter()
        .filter(|e| e.is_back)
        .map(|e| (e.from, e.to))
        .collect();

    // 1. BFS layer assignment (ignore back edges)
    let mut layer = vec![usize::MAX; n];
    layer[0] = 0;
    let mut q = std::collections::VecDeque::new();
    q.push_back(0usize);
    while let Some(u) = q.pop_front() {
        for e in &cfg.edges {
            if e.from == u && !back_set.contains(&(u, e.to)) {
                let c = layer[u] + 1;
                if c < layer[e.to] {
                    layer[e.to] = c;
                    q.push_back(e.to);
                }
            }
        }
    }
    for l in layer.iter_mut() {
        if *l == usize::MAX {
            *l = n;
        }
    }
    let max_layer = *layer.iter().max().unwrap_or(&0);

    // 2. Group by layer
    let mut groups: Vec<Vec<usize>> = vec![vec![]; max_layer + 1];
    for (i, &l) in layer.iter().enumerate() {
        if l <= max_layer {
            groups[l].push(i);
        }
    }

    // 3. Barycenter crossing reduction — snapshot adjacency to avoid borrow conflict
    let fwd_adj: Vec<Vec<usize>> = (0..n)
        .map(|u| {
            cfg.edges
                .iter()
                .filter(|e| !e.is_back && e.from == u)
                .map(|e| e.to)
                .collect()
        })
        .collect();
    let rev_adj: Vec<Vec<usize>> = (0..n)
        .map(|v| {
            cfg.edges
                .iter()
                .filter(|e| !e.is_back && e.to == v)
                .map(|e| e.from)
                .collect()
        })
        .collect();

    for _ in 0..2 {
        // top-down
        for l in 1..=max_layer {
            let prev: Vec<usize> = groups[l - 1].clone();
            let prev_pos: std::collections::HashMap<usize, usize> =
                prev.iter().enumerate().map(|(i, &v)| (v, i)).collect();
            groups[l].sort_by(|&a, &b| {
                let bc = |u: usize| -> f64 {
                    let ps: Vec<f64> = rev_adj[u]
                        .iter()
                        .filter_map(|p| prev_pos.get(p).map(|&i| i as f64))
                        .collect();
                    if ps.is_empty() {
                        u as f64
                    } else {
                        ps.iter().sum::<f64>() / ps.len() as f64
                    }
                };
                bc(a)
                    .partial_cmp(&bc(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        // bottom-up
        if max_layer > 0 {
            for l in (0..max_layer).rev() {
                let next: Vec<usize> = groups[l + 1].clone();
                let next_pos: std::collections::HashMap<usize, usize> =
                    next.iter().enumerate().map(|(i, &v)| (v, i)).collect();
                groups[l].sort_by(|&a, &b| {
                    let bc = |u: usize| -> f64 {
                        let ps: Vec<f64> = fwd_adj[u]
                            .iter()
                            .filter_map(|s| next_pos.get(s).map(|&i| i as f64))
                            .collect();
                        if ps.is_empty() {
                            u as f64
                        } else {
                            ps.iter().sum::<f64>() / ps.len() as f64
                        }
                    };
                    bc(a)
                        .partial_cmp(&bc(b))
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
    }

    // 4. Assign coordinates
    let mut layer_y: Vec<f64> = vec![0.0; max_layer + 1];
    let mut y_acc = SVG_PAD;
    for l in 0..=max_layer {
        layer_y[l] = y_acc;
        let max_h = groups[l]
            .iter()
            .map(|&i| node_h(&cfg.nodes[i]))
            .fold(NODE_H_BASE, f64::max);
        y_acc += max_h + LAYER_GAP;
    }

    let mut result = vec![
        LNode {
            idx: 0,
            layer: 0,
            x: 0.0,
            y: 0.0,
            h: NODE_H_BASE
        };
        n
    ];
    for (l, group) in groups.iter().enumerate() {
        for (pos, &idx) in group.iter().enumerate() {
            result[idx] = LNode {
                idx,
                layer: l,
                x: SVG_PAD + pos as f64 * (NODE_W + NODE_GAP),
                y: layer_y[l],
                h: node_h(&cfg.nodes[idx]),
            };
        }
    }
    result
}

// ── SVG generation ───────────────────────────────────────────────────────────
fn render_svg(cfg: &CfgGraphData) -> String {
    let layout = compute_layout(cfg);
    if layout.is_empty() {
        return "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"220\" height=\"60\"><text x=\"12\" y=\"32\" fill=\"#8d97a5\" font-size=\"13\" font-family=\"monospace\">Empty CFG</text></svg>".to_string();
    }

    let max_x = layout.iter().map(|n| n.x + NODE_W).fold(0.0_f64, f64::max) + SVG_PAD * 2.0;
    let max_y = layout.iter().map(|n| n.y + n.h).fold(0.0_f64, f64::max) + SVG_PAD * 2.0;

    let mut out = String::with_capacity(8192);
    out.push_str(&format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{max_x:.0}\" height=\"{max_y:.0}\" style=\"overflow:visible\">"));

    // Defs: arrow markers
    out.push_str("<defs>");
    for (id, col) in [
        ("ag", "#8d97a5"),
        ("ag2", "#6b7785"),
        ("ag_t", "#4ec97b"),
        ("ag_f", "#f47067"),
        ("ag_r", "#c099ff"),
        ("ag_i", "#ffb347"),
    ] {
        out.push_str(&format!(
            "<marker id=\"{id}\" markerWidth=\"7\" markerHeight=\"6\" refX=\"6\" refY=\"3\" orient=\"auto\"><polygon points=\"0 0,7 3,0 6\" fill=\"{col}\" opacity=\"0.85\"/></marker>"
        ));
    }
    out.push_str("</defs>");

    // Background
    out.push_str(&format!(
        "<rect width=\"{max_x:.0}\" height=\"{max_y:.0}\" fill=\"#0d0d14\" rx=\"0\"/>"
    ));

    // ── Edges ─────────────────────────────────────────────────────────────
    for edge in &cfg.edges {
        let Some(from) = layout.get(edge.from) else {
            continue;
        };
        let Some(to) = layout.get(edge.to) else {
            continue;
        };

        let (stroke, marker, label) = match &edge.kind {
            CfgEdgeKind::ConditionalTrue => ("#4ec97b", "ag_t", "T"),
            CfgEdgeKind::ConditionalFalse => ("#f47067", "ag_f", "F"),
            CfgEdgeKind::Unconditional => ("#8d97a5", "ag", ""),
            CfgEdgeKind::Fallthrough => ("#6b7785", "ag2", ""),
            CfgEdgeKind::Return => ("#c099ff", "ag_r", "ret"),
            CfgEdgeKind::Indirect => ("#ffb347", "ag_i", "ind"),
        };

        let dash = if edge.is_back {
            " stroke-dasharray=\"5,3\""
        } else {
            ""
        };

        if edge.is_back {
            let x1 = from.x + NODE_W;
            let y1 = from.y + from.h / 2.0;
            let x2 = to.x + NODE_W;
            let y2 = to.y + to.h / 2.0;
            let cx = x1.max(x2) + BACK_OFFSET + 20.0;
            out.push_str(&format!(
                "<path d=\"M{x1:.1},{y1:.1} C{cx:.1},{y1:.1} {cx:.1},{y2:.1} {x2:.1},{y2:.1}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"1.5\"{dash} stroke-opacity=\"0.7\" marker-end=\"url(#{marker})\"/>"
            ));
        } else {
            let x1 = from.x + NODE_W / 2.0;
            let y1 = from.y + from.h;
            let x2 = to.x + NODE_W / 2.0;
            let y2 = to.y;
            let mid_y = (y1 + y2) / 2.0;
            out.push_str(&format!(
                "<path d=\"M{x1:.1},{y1:.1} C{x1:.1},{mid_y:.1} {x2:.1},{mid_y:.1} {x2:.1},{y2:.1}\" fill=\"none\" stroke=\"{stroke}\" stroke-width=\"1.5\"{dash} stroke-opacity=\"0.85\" marker-end=\"url(#{marker})\"/>"
            ));
        }

        if !label.is_empty() {
            let lx = from.x + NODE_W / 2.0 + 6.0;
            let ly = from.y + from.h + 12.0;
            out.push_str(&format!(
                "<text x=\"{lx:.1}\" y=\"{ly:.1}\" fill=\"{stroke}\" font-size=\"9\" font-family=\"monospace\" opacity=\"0.9\">{label}</text>"
            ));
        }
    }

    // ── Nodes ──────────────────────────────────────────────────────────────
    for ln in &layout {
        let node = &cfg.nodes[ln.idx];
        let x = ln.x;
        let y = ln.y;
        let h = ln.h;

        let (bg, border, text_col) = if node.is_entry {
            ("#12233a", "#3d8ef0", "#8dc8ff")
        } else if node.is_exit {
            ("#27142a", "#b07af0", "#d4aaff")
        } else {
            ("#161822", "#2e3347", "#9ea8b3")
        };

        out.push_str(&format!(
            "<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{NODE_W}\" height=\"{h:.1}\" rx=\"5\" fill=\"{bg}\" stroke=\"{border}\" stroke-width=\"1.5\"/>"
        ));

        // Address
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{text_col}\" font-size=\"11\" font-weight=\"600\" font-family=\"monospace\" text-anchor=\"middle\">{}</text>",
            x + NODE_W / 2.0,
            y + 17.0,
            node.label(),
        ));

        // Badge
        let badge = if node.is_entry && node.is_exit {
            "ENTRY / EXIT".to_string()
        } else if node.is_entry {
            "ENTRY".to_string()
        } else if node.is_exit {
            "EXIT".to_string()
        } else {
            format!("{} ops", node.op_count)
        };
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{text_col}\" font-size=\"9\" font-family=\"monospace\" text-anchor=\"middle\" opacity=\"0.6\">{badge}</text>",
            x + NODE_W / 2.0,
            y + h - 9.0,
        ));

        // BB index
        out.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" fill=\"{border}\" font-size=\"8\" font-family=\"monospace\" text-anchor=\"end\" opacity=\"0.55\">BB{}</text>",
            x + NODE_W - 5.0,
            y + 10.0,
            node.index,
        ));
    }

    out.push_str("</svg>");
    out
}

// ── Component ────────────────────────────────────────────────────────────────

#[component]
pub fn CfgView() -> Element {
    let state = use_app_state();
    let cfg_opt = state.read().current_cfg.clone();

    let Some(cfg) = cfg_opt else {
        return rsx! {
            div { class: "cfg-empty",
                span { class: "cfg-empty-title", "CFG Viewer" }
                span { class: "cfg-empty-sub", "Select a function to visualise its control-flow graph." }
            }
        };
    };

    let mut scale = use_signal(|| 1.0_f64);
    let mut offset_x = use_signal(|| 0.0_f64);
    let mut offset_y = use_signal(|| 0.0_f64);
    let mut panning = use_signal(|| false);
    let mut pan_xy = use_signal(|| (0.0_f64, 0.0_f64));

    let stats = format!(
        "{} blocks  \u{00B7}  {} edges  \u{00B7}  cyclomatic {}",
        cfg.block_count, cfg.edge_count, cfg.cyclomatic
    );

    let svg_html = render_svg(&cfg);

    let sc = *scale.read();
    let ox = *offset_x.read();
    let oy = *offset_y.read();

    rsx! {
        div { class: "cfg-panel",
            div { class: "cfg-toolbar",
                span { class: "cfg-stats", "{stats}" }
                div { class: "cfg-toolbar-actions",
                    button {
                        class: "cfg-btn",
                        onclick: move |_| {
                            scale.set(1.0);
                            offset_x.set(0.0);
                            offset_y.set(0.0);
                        },
                        "Reset"
                    }
                    button {
                        class: "cfg-btn",
                        onclick: move |_| {
                            let v = *scale.read();
                            scale.set((v * 1.25).min(5.0));
                        },
                        "+"
                    }
                    button {
                        class: "cfg-btn",
                        onclick: move |_| {
                            let v = *scale.read();
                            scale.set((v / 1.25).max(0.1));
                        },
                        "\u{2212}"
                    }
                }
            }

            div {
                class: "cfg-canvas",
                onwheel: move |e| {
                    let dy = e.delta().strip_units().y;
                    let f  = if dy < 0.0 { 1.1 } else { 1.0 / 1.1 };
                    let v  = *scale.read();
                    scale.set((v * f).clamp(0.1, 6.0));
                },
                onmousedown: move |e| {
                    panning.set(true);
                    let c = e.client_coordinates();
                    pan_xy.set((c.x as f64, c.y as f64));
                },
                onmousemove: move |e| {
                    if *panning.read() {
                        let c  = e.client_coordinates();
                        let ps = *pan_xy.read();
                        let dx = c.x as f64 - ps.0;
                        let dy = c.y as f64 - ps.1;
                        let ox_new = *offset_x.read() + dx;
                        let oy_new = *offset_y.read() + dy;
                        offset_x.set(ox_new);
                        offset_y.set(oy_new);
                        pan_xy.set((c.x as f64, c.y as f64));
                    }
                },
                onmouseup:    move |_| panning.set(false),
                onmouseleave: move |_| panning.set(false),

                div {
                    style: "transform: translate({ox}px, {oy}px) scale({sc}); transform-origin: 0 0; cursor: grab; display: inline-block;",
                    dangerous_inner_html: "{svg_html}",
                }
            }
        }
    }
}
