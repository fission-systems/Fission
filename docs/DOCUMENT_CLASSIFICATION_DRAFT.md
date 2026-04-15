# Fission Documentation Classification Draft

## Purpose

This draft defines where each document type should live so that repository contracts stay close to code while user-facing guides can evolve quickly in GitHub Wiki.

## Classification Table (Draft)

| Document Category | Canonical Location | Why Here | Sync Rule | Owner |
| --- | --- | --- | --- | --- |
| Architecture contracts | Repository (`docs/` or crate-local docs) | Must be reviewed with code and versioned atomically | PR required in main repo | Core maintainers |
| Build/release engineering rules | Repository (`.github/workflows/`, `docs/`) | CI/CD and release semantics are code-coupled | PR required in main repo | Platform maintainers |
| API behavior and compatibility notes | Repository (crate docs, `README.md`) | Needs strict traceability to commits | PR required in main repo | Crate owners |
| Changelog and release notes | Repository (`docs/changelog/`) | Must remain immutable and tag-aligned | Update per release PR | Release manager |
| User tutorials and quickstarts | Wiki | Frequent edits and examples by broader team | Wiki direct edit allowed | DevRel / maintainers |
| Troubleshooting and FAQ | Wiki | Fast operational updates without code PR overhead | Wiki direct edit allowed | Maintainers / community |
| Benchmark interpretation guides | Wiki (summary) + repository (raw artifacts) | Narrative changes often, raw data must stay auditable | Keep links to artifact paths | Perf owners |
| Onboarding/runbooks | Wiki | Team process content changes frequently | Wiki direct edit allowed | Team leads |

## Migration Priority (Draft)

1. Move user-focused guides from repository markdown to Wiki pages.
2. Keep architecture and contract docs in repository.
3. Keep bilingual release changelog in repository.
4. Add cross-links between repository docs and corresponding Wiki pages.

## Governance Rules

- If a document defines behavior that can break builds/tests, keep it in repository.
- If a document is instructional and high-churn, keep it in Wiki.
- Every Wiki page should include a backlink to repository root and related crate path.
- Repository docs should include a Wiki hub link for discoverability.
