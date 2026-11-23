# Issue label taxonomy

**Last verified:** 2026-05-02

GitHub labels are configured in repository settings; this document is the **recommended taxonomy** so triage stays consistent. Maintainers should mirror these names when creating labels.

## Prefix conventions

| Prefix | Meaning | Examples |
|--------|---------|----------|
| `area:` | Subsystem / crate neighborhood | `area:pcode`, `area:loader`, `area:cli`, `area:emulator`, `area:dynamic`, `area:benchmark` |
| `kind:` | Issue type | `kind:bug`, `kind:enhancement`, `kind:task`, `kind:docs`, `kind:process` |
| `priority:` | Urgency | `priority:P0`, `priority:P1`, `priority:P2` |
| `status:` | Workflow | `status:triage`, `status:blocked`, `status:needs-info`, `status:good-first-issue` |

## Good first issue

Use **`status:good-first-issue`** for bounded tasks that:

- Touch at most one area prefix,
- Include a suggested file entrypoint (`docs/PROJECT_MAP.md`),
- Link relevant tests (`cargo nextest run -p …`).

Examples:

- Add regression fixture in the owning crate's test fixtures with documented build steps.
- Improve CLI `--help` text or docs linkbacks only.

## Title hints

| Pattern | Example |
|---------|---------|
| `area:` prefix in title optional but helpful | `[area:loader] PE TLS directory mis-parsed for …` |
| Include observable symptom | `CLI decomp emits empty body for recursive tail call` |

## Related docs

- Semantic ownership vs labels: [`AGENTS.md`](https://github.com/fission-systems/Fission/blob/main/AGENTS.md), [`docs/PROJECT_MAP.md`](../PROJECT_MAP.md)
- Security-sensitive reports: [`SECURITY.md`](https://github.com/fission-systems/Fission/blob/main/SECURITY.md) (do **not** drive coordinated disclosure via public issues).
