# Fission Wiki Initial Table of Contents (Draft)

Wiki URL:

- Home: [https://github.com/sjkim1127/Fission/wiki](https://github.com/sjkim1127/Fission/wiki)
- Git: [https://github.com/sjkim1127/Fission.wiki.git](https://github.com/sjkim1127/Fission.wiki.git)

## Top-Level Structure

1. Home
2. Getting Started
3. User Guides
4. Reverse Engineering Workflows
5. Benchmark & Quality Playbooks
6. Troubleshooting
7. Contributor Onboarding
8. FAQ
9. Glossary
10. Release Notes Index (links to repository changelog)

## Suggested Page Tree

### 1) Home

- Project overview
- Current maturity and scope boundaries
- Quick links to CLI, desktop, and repository docs

### 2) Getting Started

- Installation prerequisites
- First decompilation walkthrough
- Common command patterns

### 3) User Guides

- CLI usage guide
- Tauri desktop workflow guide
- Output reading guide (NIR/HIR/structured output)

### 4) Reverse Engineering Workflows

- Function triage workflow
- Unsupported-path fallback workflow
- Comparative workflow against Ghidra outputs

### 5) Benchmark & Quality Playbooks

- Running `nir-check`
- Reading automation artifacts
- Benchmark scenario templates and interpretation

### 6) Troubleshooting

- Build failures by platform
- Runtime/analysis mismatch checklist
- Performance regression triage

### 7) Contributor Onboarding

- Repository map
- Ownership boundaries by crate
- PR checklist and review expectations

### 8) FAQ

- Scope and non-goals
- Supported binary formats and caveats
- Why Rust-first and how Ghidra is used

### 9) Glossary

- Terminology for NIR/HIR/CFG/structuring
- Internal naming conventions

### 10) Release Notes Index

- Link to `docs/changelog/CHANGELOG.md`
- Link to `docs/changelog/CHANGELOG.ko.md`

## Maintenance Notes

- Keep Wiki pages short and task-oriented.
- Link back to repository files for canonical implementation details.
- Avoid duplicating normative contracts in Wiki; reference repository instead.
