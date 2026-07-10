# Audit Reports

This directory stores generated or hand-curated architecture/process audit
reports. Reports here are evidence for review, not production inputs.

Expected report families:

- benchmark smell scans,
- AI prompt leak scans,
- architecture isolation scans,
- ISA / semantic debt inventories (ADR 0009 backlog),
- six-month regression replay summaries,
- metric gaming / Goodhart checks.

| Report | Notes |
|--------|--------|
| [2026-07-10-decompiler-problem-inventory.md](2026-07-10-decompiler-problem-inventory.md) | Current quality problem families (F1–F6) and ROI ranking |
| [2026-07-10-isa-semantic-debt-inventory.md](2026-07-10-isa-semantic-debt-inventory.md) | P0 OK/DEBT/ENV table for NIR ISA gates and emergency passes |
| [2026-07-04-arch-isolation-scan.md](2026-07-04-arch-isolation-scan.md) | Noisy token scan of arch/register names in NIR |
| [2026-07-04-benchmark-smell-scan.md](2026-07-04-benchmark-smell-scan.md) | Benchmark smell scan |
| [2026-07-04-metric-gaming.md](2026-07-04-metric-gaming.md) | Metric gaming / Goodhart checks |
| [2026-07-04-nir-boundary-scan.md](2026-07-04-nir-boundary-scan.md) | NIR boundary scan |
| [2026-07-04-regression-replay.md](2026-07-04-regression-replay.md) | Regression replay summary |
