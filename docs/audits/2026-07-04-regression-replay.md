# Regression Replay Plan

- Repo: `/Users/sjkim1127/Fission`
- Since: `6.months`
- Candidate commits: `302`

## Method

For each candidate commit, run the selected benchmark subset at the parent
commit and at the candidate commit. Count newly passing rows, newly failing
rows, and net semantic delta. Keep artifacts under `docs/audits/` or an
external run directory; do not update dashboards from this audit.

## Candidate Commits

| Date | Commit | Subject |
|---|---|---|
| 2026-07-04 | `4a48b74f7c10` | Improve decompiler semantic recovery for 0.1.2 |
| 2026-07-03 | `237e5839e4f7` | Fix conditional materialization for clamp-like returns |
| 2026-07-02 | `d17a0ca63053` | ci: fix heavy validation runner by removing benchmark and corpus dependencies |
| 2026-07-01 | `c68446a70a2e` | fix: clippy warnings and unused muts |
| 2026-07-01 | `955ada864540` | style: fix formatting |
| 2026-07-01 | `d83c18b650e5` | feat(decompiler): add write API to FactStore and DecompContext for feedback loop |
| 2026-07-01 | `04ee10e1cfcc` | refactor(core): move CallingConvention from pcode to core to fix dependency cycle |
| 2026-07-01 | `54457e954c7d` | refactor(decompiler): Phase 1 — DecompContext struct + pipeline translation boundary consolidation |
| 2026-07-01 | `1ba05f057a95` | arch: anti-overfitting pass-gate architecture for NIR structuring pipeline |
| 2026-07-01 | `c59cfcaabda8` | ci: build nir fixture at manifest path |
| 2026-07-01 | `74597ee7232f` | fix: ignore AppleDouble sleigh metadata files |
| 2026-07-01 | `5eeb26656fcc` | fix: ignore AppleDouble ghidra metadata files |
| 2026-07-01 | `4bc0a850fa12` | fix(deps): revert gimli 0.34 to 0.32 to fix compilation errors |
| 2026-07-01 | `c6b703a9a548` | fix(ci): fix yaml indentation in reusable-benchmark |
| 2026-07-01 | `55814befd039` | fix(ci): fetch utils as composite action |
| 2026-07-01 | `c541d8e81df8` | fix(ci): resolve clippy warnings and disable missing fixture tests |
| 2026-06-30 | `a854c2188177` | chore: remove utils/ from GitHub entirely |
| 2026-06-30 | `aa7dc9067bb0` | fix: replace white background with dark (#0d1117) in icon.png |
| 2026-06-30 | `4e1b2351f595` | docs: complete README redesign + new icon |
| 2026-06-30 | `adf7427d6087` | chore: major repo cleanup — LFS, benchmark/, vendor MANIFEST |
| 2026-06-30 | `5c5d1e28831f` | chore: keep only scripts/test/ in git tracking |
| 2026-06-30 | `8498b42e02c4` | chore: migrate benchmark/binary/ to LFS + fix CI selective pull strategy |
| 2026-06-30 | `d4c064fb0fca` | chore: remove utils/ and benchmark/binary/ from git tracking |
| 2026-06-30 | `0d9554c655e5` | ci: add lfs: false/true to all actions/checkout steps |
| 2026-06-30 | `eaaf39f226ed` | chore: migrate large binary assets to Git LFS (~536 MB) |
| 2026-06-30 | `80fa47845b93` | refactor(automation): remove source-semantic-check subcommand |
| 2026-06-30 | `0f071af5f475` | ci: overhaul CI/CD pipeline with conservative validation |
| 2026-06-25 | `0161802221d3` | fix(ci): exclude fission-dioxus from clippy on Linux to avoid webview dependencies |
| 2026-06-25 | `c1ba163162d5` | fix(ci): run builds and tests in parallel, do not skip them on lint/security failure |
| 2026-06-24 | `f4726552da31` | fix(ci): Resolve CI/CD failures across workspace and workflows |
| 2026-06-24 | `4e79804494be` | fix(ci): Resolve CI failures in lint and security steps |
| 2026-06-24 | `ffdcb47b92a6` | ci: lighten fast gate, remove heavy push trigger, add gitattributes |
| 2026-06-16 | `73424aa5a18d` | fix(pcode): improve intra-block conditional move structuring and cache budget rejections |
| 2026-06-06 | `37033e815347` | chore: clean up root directory clutter |
| 2026-06-06 | `66dab7e65fbb` | Align loader, static, and sleigh CFG fact paths for Ghidra parity. |
| 2026-06-05 | `5f4ac85911f9` | Replace register_map with SLA-first RegisterNamer and cspec register model. |
| 2026-06-05 | `08e406a17b39` | Fix fill_matrix nested loop recovery and extend NIR normalize rules. |
| 2026-06-04 | `6fa118771c76` | feat(pspec): Ghidra-style .pspec runtime integration |
| 2026-06-04 | `e7ebd897da38` | nir: .cspec-based dynamic ABI resolution (Ghidra-style) |
| 2026-06-03 | `62dd550fc8fe` | chore: update dependencies and fix cargo deny advisories |
| 2026-06-02 | `485b89b0c46f` | printer: fix invalid C for pointer arithmetic and ptr-to-int casts |
| 2026-06-02 | `8cd139724cfa` | Fix memory heritage dropping statements and switch chain panic |
| 2026-06-02 | `104f8b7749f0` | fix(pcode): Patch severe CFG structuring and normalization bugs |
| 2026-06-02 | `cbf7ed25dc65` | fix(tui): patch 6 latent bugs found in safety review |
| 2026-06-02 | `5a2e29efd91e` | fix(tui): never auto-switch view — chat stream must never be interrupted |
| 2026-06-02 | `9cfb6c424c58` | fix(fission-tui): Fix scroll tracking and add /export command |
| 2026-06-01 | `f671413adc50` | fix(structuring,normalize): resolve TraceDAG timeouts and I/O bottlenecks |
| 2026-06-01 | `891410f1eb8a` | fix(nir): fix ZExt passthrough accumulator naming in loop bodies |
| 2026-06-01 | `065cd351fe43` | pcode: resolve loop-carried register materialization and xVar fallback issues |
| 2026-05-29 | `7dd6865a89fb` | feat(benchmark): add comprehensive speed benchmark script and fix CLI callgraph routing |
| 2026-05-29 | `f95c9dd52617` | fix(nir/builder): x86-64 ZExt passthrough look-through for loop accumulator naming |
| 2026-05-28 | `1631b97221db` | sigs: add ntoskrnl kernel API signatures for call argument pruning |
| 2026-05-27 | `fac064534514` | feat(signatures): expand GDT signature extraction and fix phi_recovery borrow |
| 2026-05-26 | `df6f42d56494` | fix(decompiler): restore address-of operator (&) for global symbol constants and fix type inference |
| 2026-05-26 | `7335e2d0837e` | fix(decompiler): resolve orphan goto fallback and missing temporary declarations |
| 2026-05-26 | `d2597cbac13d` | structuring(m3): FAS virtualization, switch fallthrough, SAILR H2 postdom exit selection |
| 2026-05-26 | `2ca98397a33b` | refactor(structuring): prevent pathological hangs via multi-tier auto-budgeting and SESE time boundaries |
| 2026-05-25 | `fc046b2c08d9` | feat(nir): implement ActionLikelyTrash pass for caller-saved register alias elimination |
| 2026-05-24 | `5d5b8df1d4c8` | Fix bit_reverse loop off-by-one and bitwise ops recovery |
| 2026-05-24 | `f9e6576483fc` | feat: implement memory heritage solver, type propagator, and rule normalizer passes, fix loader profiling & spawn overhead |
| 2026-05-22 | `0662b00ba3ec` | sleigh: remove legacy varnode_from_fixed_handle fallback, use pure ConstructTpl template resolution |
| 2026-05-21 | `52cd37260a15` | fix(builder): reuse wide aliases for partial gpr updates |
| 2026-05-21 | `add12d208638` | fix(builder): prevent wide register binding from hijacking narrow loop-carried update |
| 2026-05-20 | `d5427e320a9c` | feat(windows-debugger): implement anti-debug, memory map, SEH, PEB/TEB, module enumeration |
| 2026-05-20 | `19c46de99f4c` | refactor: modularize CLI args and merge unpacker into debugger |
| 2026-05-19 | `a930ceadb13f` | refactor(dynamic): switch exclusively to Sleigh for instruction decoding |
| 2026-05-19 | `3efa7e73a66e` | feat(dynamic): integrate fission-sleigh for instruction decoding and disassembly |
| 2026-05-19 | `db4f7f3fe303` | feat(dynamic): complete Windows debugger Phase 1 — event processing, thread/module tracking, step-over |
| 2026-05-19 | `5b2ee1ba4344` | feat(automation): make source-semantic-check the primary subcommand |
| 2026-05-19 | `acd0df191e63` | refactor(nir): extract test modules from 5 large files into separate *_tests.rs files |
| 2026-05-19 | `42b6cded6030` | refactor(nir): extract rendering pipeline, ABI helper, and wrapper summary into dedicated modules |
| 2026-05-17 | `5780e9cc4743` | Silence merge trial rejection debug logs |
| 2026-05-16 | `bc08d561005c` | Potential fix for pull request finding |
| 2026-05-16 | `fb201e892b43` | Potential fix for pull request finding |
| 2026-05-15 | `d48556e53bd7` | Merge pull request #60 from sjkim1127/codex/source-semantic-nir-quality |
| 2026-05-15 | `0074fc0a0efc` | Improve NIR source semantic recovery |
| 2026-05-15 | `1d7b4c45eefe` | Add source semantic score sensitivity metrics |
| 2026-05-15 | `92fad63f8800` | Batch prewarm source semantic decompilation |
| 2026-05-15 | `8b37afc1e3f4` | Add source semantic readiness metrics |
| 2026-05-15 | `07d5a2495525` | Fix cross-block call result register reads |
| 2026-05-15 | `5266701eac4d` | Fix low-bit Rust-Sleigh decode extent |
| 2026-05-15 | `f20e4dc0b79a` | Add source semantic quality metrics |
| 2026-05-15 | `d1bf74999173` | Account for same-source inlining in semantic scoring |
| 2026-05-15 | `29eed767d31e` | Add roadmap source semantic diagnostics |
| 2026-05-15 | `2d6cfb1f20f3` | Extract rendered signatures in source semantic scoring |
| 2026-05-15 | `0ce2d24cf229` | Add source semantic coverage diagnostics |
| 2026-05-14 | `6fe0dbc3b530` | Add source semantic focus area metrics |
| 2026-05-14 | `27141f0dc346` | Expand source semantic benchmark metrics |
| 2026-05-14 | `83c31a62b3a3` | Fix ARM Thumb global relocations |
| 2026-05-14 | `75c76124f75d` | Add source semantic triage metrics |
| 2026-05-14 | `2234a7151502` | Fix ARM Thumb operand expression offsets |
| 2026-05-14 | `5fb1b96a0d15` | Add source semantic coverage breakdown metrics |
| 2026-05-14 | `30f7dac837a1` | Add source semantic triage metrics |
| 2026-05-14 | `80451ddd2bfa` | Name SLEIGH build debug op count |
| 2026-05-14 | `2e6d6abe5c6a` | Expose source semantic triage metrics |
| 2026-05-14 | `afa20ac891b4` | Expose source semantic scoring denominators |
| 2026-05-14 | `9e3a5a3ddcf4` | Add source semantic denominator metrics |
| 2026-05-14 | `446cc615e0a8` | Add source semantic triage metrics |
| 2026-05-14 | `e998ae0c88aa` | Add source semantic gap metrics |
| 2026-05-14 | `24e9377bbd29` | Version source semantic debug evidence cache |
| 2026-05-14 | `0a14064b7a7b` | Add source semantic diagnostic metrics |
| 2026-05-14 | `96a18f1cf90c` | Add source semantic benchmark filters |
| 2026-05-14 | `8484cbd0d592` | Lock source semantic missing-row scoring contract |
| 2026-05-14 | `0f9336bc9bab` | Gate source semantic SLEIGH template sources |
| 2026-05-13 | `7ae17d8aba44` | docs(benchmark): fold debug canary command details |
| 2026-05-13 | `0acbd6203472` | docs(github): add benchmark regression issue form |
| 2026-05-13 | `9af9fbf9b8a0` | Merge pull request #56 from sjkim1127/bench/expand-source-semantic-v1 |
| 2026-05-13 | `37598a41c067` | Merge pull request #55 from sjkim1127/bench/expand-source-semantic-v1 |
| 2026-05-13 | `e6c2d1aceffd` | docs(readme): surface source semantic feature-shape canaries |
| 2026-05-13 | `b80a9aef8a1b` | Merge pull request #54 from sjkim1127/bench/expand-source-semantic-v1 |
| 2026-05-13 | `0eb6d5a02db0` | docs(source-semantic): document feature-shape canaries |
| 2026-05-13 | `4cb0ed80c309` | bench(source-semantic): add feature-shape canary manifest |
| 2026-05-11 | `dd7c721ed045` | Prefer literal source symbol matches in semantic benchmark |
| 2026-05-11 | `8acf89f2240e` | Add focused AArch64 semantic benchmark lane |
| 2026-05-11 | `cfe44ef61a39` | Prefer exported operand debug values over fixed fallback |
| 2026-05-11 | `b448d93c18ee` | Constrain exported fixed display fallback |
| 2026-05-11 | `5fed950f936b` | Prefer SLA display operands for exported debug values |
| 2026-05-11 | `8fb753ee3a01` | Constrain exported display fixups to SLA operands |
| 2026-05-11 | `7730661c854b` | Refresh SLEIGH regression expectations |
| 2026-05-09 | `9e3044450338` | Match Ghidra offset-plus shift semantics |
| 2026-05-09 | `75d27c62f322` | Track source semantic benchmark history |
| 2026-05-09 | `4cae0dd2ee8d` | Materialize regression debug triage in source benchmark |
| 2026-05-09 | `c12ff098ac49` | Extend source semantic benchmark triage |
| 2026-05-09 | `9aade4fdfffc` | Stop pattern expressions using debug operands |
| 2026-05-09 | `b2fef6652c1d` | Remove stale fallback debug affordance |
| 2026-05-09 | `8ced2912f1e4` | Enhance source semantic debug triage cache reporting |
| 2026-05-09 | `395a8517de20` | Enhance source semantic benchmark triage artifacts |
| 2026-05-09 | `087b61486e3e` | Enhance source semantic benchmark history triage |
| 2026-05-09 | `c76ef22a8fc3` | Aggregate source benchmark debug evidence |
| 2026-05-09 | `b31f6a5122b7` | Materialize source benchmark debug bundles |
| 2026-05-09 | `cd4c4fe287e4` | Persist source semantic run history |
| 2026-05-08 | `913a30b9a5b9` | Expose Rust-Sleigh pcode topology in debug bundles |
| 2026-05-08 | `1120c12a296d` | Persist source semantic decompile cache |
| 2026-05-08 | `893c051f93ce` | Add source semantic benchmark comparisons |
| 2026-05-08 | `453ec8feca80` | Add percent output and parallel jobs to source semantic benchmark |
| 2026-05-07 | `b6a9101d7eb6` | Fix register alias def-use lowering |
| 2026-05-06 | `0c6db9e23689` | Add source semantic benchmark |
| 2026-05-06 | `0998fd2108c3` | fix(pcode): narrow zero-extended return widths |
| 2026-05-06 | `f5222af252da` | fix(pcode): structure intra-instruction return copies |
| 2026-05-06 | `a3c8fe3c5158` | fix(pcode): recover rust-sleigh return register values |
| 2026-05-06 | `29118f1a7aa8` | fix(pcode): narrow runtime register param recovery |
| 2026-05-06 | `8b5fbcc8c513` | fix(pcode): recover rust-sleigh entry register params |
| 2026-05-06 | `6a3a8ecb5fdc` | fix(pcode): map x64 subregister aliases to abi params |
| 2026-05-06 | `2758d893bfe8` | fix(pcode): treat flag helpers as guarded-tail pure calls |
| 2026-05-06 | `fcd5f76415da` | fix(loader): merge PE pdata extents into COFF symbols |
| 2026-05-05 | `e66f1560ac90` | feat(static): external symbol index, function provenance, API import lookup |
| 2026-05-04 | `56a6783bd052` | ci(cd): trigger releases on bare semver tags as well as v-prefix |
| 2026-05-04 | `b872da995c36` | refactor(fission-dynamic): unify OS debug under platform; rename ttd→timeline |
| 2026-05-04 | `834e341ef1af` | Canonical WinAPI/type JSON under utils/signatures; drop crate-local data |
| 2026-05-04 | `5e182988091b` | Remove fission-analysis facade; move benchmarks to fission-static |
| 2026-05-04 | `74f0846777a7` | refactor(cli,core): rust_decomp modules, rust_sleigh evidence, legacy postprocess quarantine |
| 2026-05-04 | `0e549bf5c280` | refactor: decouple crate tests from benchmark fixtures |
| 2026-05-03 | `a16c028a0044` | Fix sqlite SLEIGH raw p-code canaries |
| 2026-05-03 | `c3b211f685f2` | fix(ci): PE smoke picks add/main over MinGW CRT symbols |
| 2026-05-03 | `784eae8f9409` | fix(ci): install MinGW via apt-get in cli-smoke (avoid cache PATH gap) |
| 2026-05-03 | `765a8a9b20a6` | fix(fission-tauri): sync package-lock for @emnapi/* (npm ci on CI) |
| 2026-05-03 | `b3de2fa2f2f4` | fix(tauri-ui): bump @vitejs/plugin-react for Vite 8 peer range |
| 2026-05-02 | `51a7b7f7960f` | fix: sync cargo-audit with deny baselines; serialize defuse env tests |
| 2026-05-02 | `c6c7ea4535e6` | fix(loader): satisfy clippy manual_checked_ops for ELF sym counts |
| 2026-05-02 | `06e8bc75959e` | fix(ci): unblock clippy -D warnings on stable |
| 2026-05-02 | `d903975cb135` | fix(deny): allow AGPL/MPL licenses and declare fission-sleigh license |
| 2026-05-02 | `7001cbf0c62b` | ci: build smoke corpus PE fixtures with MinGW for benchmark job |
| 2026-05-02 | `7f48dd495805` | fix(loader): use public loader API in fuzz targets |
| 2026-05-02 | `35fff09c2d26` | fix(loader): use public loader API in fuzz targets |
| 2026-05-02 | `c412a57d3708` | fix(loader): isolate fuzz crate from workspace |
| 2026-05-02 | `4564577e2081` | fix(loader): allow legacy clippy pedantic lints |
| 2026-05-02 | `ea61720ef213` | ci: bash shell + timeouts for Windows tests; fast CI uses debug |
| 2026-05-02 | `49b40d7bc16f` | ci: install MinGW via apt-get for nir-check PE fixture |
| 2026-05-02 | `e15988c81423` | fix(sleigh): executable allowlist for manifest; skip SLA tests without Ghidra |
| 2026-05-02 | `85b4451616f5` | ci: build PE fixture for nir lane sentinel manifest |
| 2026-05-02 | `82c251436d62` | fix(signatures): add GetClientRect/GetWindowRect/GetMessage* to Win32 API DB |
| 2026-05-02 | `ff0a04e3ed2b` | fix(ci): clippy-friendly architecture select + serialized sleigh discovery tests |
| 2026-05-02 | `6a249e0a5d1a` | ci: fix test exclusions and stabilize function discovery fixtures |
| 2026-05-02 | `cf0106c10b6a` | docs(ci): release gate, security policy, issue forms, third-party canary |
| 2026-05-02 | `8d66ac1cf436` | benchmark: point smoke/release/parity corpora at benchmark/binary |
| 2026-05-02 | `26a659c9fdc5` | ci: fix reusable-build-cli TARGET_DIR (bash-only; invalid PROFILE_DIR in expressions) |
| 2026-05-01 | `bdc2efb9efba` | Fix AArch64 exported handle p-code parity |
| 2026-05-01 | `bb69df633e75` | Fix ELF relocatable loader byte lookup |
| 2026-04-30 | `c2c62c5db7ea` | feat(fission-sleigh): x86-32 P-code 100% parity via reloffset propagation |
| 2026-04-30 | `a82756cd19c9` | feat(sleigh): 100% canonical parity — BRANCH/BRANCHIND/CALLIND SLA opcode alignment |
| 2026-04-25 | `05ed3d0b2355` | fix(sleigh): robust handle remapping for SLA templates with invisible operands |
| 2026-04-24 | `55361383b14b` | Cut over compiled-table semantic emission |
| 2026-04-22 | `5f05dd0c8482` | refactor: include medium binary source code in repository |
| 2026-04-22 | `eec748164e97` | feat: add comprehensive performance benchmarking dashboard with criterion.rs integration and medium binary samples |
| 2026-04-20 | `2242f9ba4f92` | Trace missing incoming semantics owners |
| 2026-04-20 | `99dcef51db60` | Trace no-prior-def incoming semantics |
| 2026-04-20 | `6117a036c269` | Add parity chain regression attribution trace |
| 2026-04-19 | `6e69aaf2253d` | Trace no-consumer suppression regressions |
| 2026-04-18 | `ffb99b2764de` | Internalize known pure helper suffix calls |
| 2026-04-18 | `26fb53a5e54a` | Subtype guarded-tail suffix side effects |
| 2026-04-18 | `36a2063615af` | Subtype guarded-tail nested suffix blockers |
| 2026-04-18 | `a9255ddb8da3` | Close self-terminal guarded tail suffixes |
| 2026-04-18 | `23d22703bf2a` | Tighten guarded-tail suffix redirect closure |
| 2026-04-18 | `e89b78042d06` | Add guarded-tail suffix tail rejection diagnostics |
| 2026-04-17 | `9a50e43693ee` | feat(nir): collapse guard-prefix sink goto chains to terminal return |
| 2026-04-17 | `aae44afb1124` | feat(nir): allow terminal-safe nested tail exit subcase in guarded-tail canonicalization |
| 2026-04-17 | `ecb785f637b6` | fix(nir): align guarded-tail verify must-emit gating |
| 2026-04-15 | `20d142c98005` | feat(nir): join-glue middle refs for guarded-tail promotion |
| 2026-04-11 | `97cf69a29fb3` | Fix benchmark direct-success reporting |
| 2026-04-11 | `91dc4acdbd2b` | Consolidate canonical semantics ownership |
| 2026-04-10 | `8b31a50f5450` | Fix virtual block structuring recovery |
| 2026-04-10 | `31457b353a2b` | Tighten call semantics and MemSSA ownership |
| 2026-04-10 | `8325b2c645ab` | Implement semantics-first decompilation core wave |
| 2026-04-10 | `4e8c16229c04` | fix(automation): restore rust-only nir-check inventory |
| 2026-04-10 | `0ceb330d178d` | refactor: NIR normalize tree, cfg_analysis split, automation report, sleigh semantic |
| 2026-04-09 | `cdc2c970d192` | Improve NIR structuring/type fixpoint and refresh 3-gate benchmarks |
| 2026-04-09 | `d758fb813bd6` | fix(structuring): require matching terminal-kind for no-exit merges |
| 2026-04-09 | `18afdab8e673` | fix(fission-pcode): refresh dominator cache after CFG mutation |
| 2026-04-09 | `20a853686325` | feat(pcode): HIR Phase 9 — SCCP, join GVN-lite, wide def-use DCE |
| 2026-04-09 | `bc9096b977d3` | feat(pcode): HIR Quality Phase 8 — RLE, branch prefix hoist, affine IV, expr_key |
| 2026-04-09 | `724d0ab8a253` | feat(pcode): HIR Quality Phase 7 — LICM, Local CSE, Sar sign propagation |
| 2026-04-09 | `4aac93e4f6ae` | feat(pcode): HIR Quality Phase 4 — use-type-infer, ptr-arith, return-type, label-inline |
| 2026-04-09 | `04a2e38211dd` | feat(pcode): HIR expressiveness phase 3 — EFLAGS recovery, prologue elimination, Cooper postdom structuring |
| 2026-04-08 | `113c81301241` | feat/x86: 4th lifter reinforcement pass — coverage ~87% → ~93% |
| 2026-04-06 | `6fd6a170e882` | sleigh/x86: tighten byte div semantics and unsupported marker schema |
| 2026-04-05 | `19d652606445` | x86 0F3A semantics and CFG branch-target diagnostics |
| 2026-04-05 | `a3eb9eec5842` | x86: expand F6 group semantics and update changelog |
| 2026-04-04 | `86ce1bfe722e` | fission-sleigh: split aarch64 semantic modules and tests |
| 2026-04-03 | `eac847b49036` | docs(changelog): add 2026-04-03 arm64 InvalidRef fix entry |
| 2026-04-03 | `831564ba3842` | docs(changelog): add 2026-04-03 arm64 InvalidRef fix entry |
| 2026-03-28 | `14cc53cdd669` | Add builder debug and stats modules |
| 2026-03-24 | `64aee7b262ca` | Fix CI workflow args and Windows decompiler target build |
| 2026-03-23 | `894f0415ac3d` | Strengthen CI pipelines and fix decompiler build invocation |
| 2026-03-23 | `7d680d74bb42` | Tighten region recovery semantics and preserve cached reject reasons |
| 2026-03-23 | `fcbbb651b005` | Align linear structuring regressions with current lowering behavior |
| 2026-03-12 | `7e6b7d2139ed` | fix(decomp): recover legacy C output from Duplicate VariablePiece failures |
| 2026-03-10 | `a9589dc59f71` | fix(gui): sync decompiler option toggles and decompile symbol interactions |
| 2026-03-10 | `22c5eb33bf8c` | feat(analysis): implement label-prefix sinking to normalize CFG |
| 2026-03-10 | `818b65f678d0` | Fix unique piece duplication in RulePieceStructure |
| 2026-03-10 | `4b02765e6b9b` | perf: finalize graceful degradation and 10s baseline |
| 2026-03-09 | `0326b9def673` | perf: fix multithread timeout (900s→26s) + dynamic worker scaling |
| 2026-03-09 | `b769e920248d` | fix: 8스레드 멀티스레드 안정화 패치 (UserPcodeOp null 체크, Sleigh 직렬화) |
| 2026-03-08 | `eac0431496a4` | fix: TypePropagator UAF 수정, Phase D Object Pool(롤백), decomp 직렬화 |
| 2026-03-07 | `29d5ccc283ce` | perf: 디컴파일러 성능 최적화 + 성공률 61%→87% 개선 |
| 2026-03-07 | `6ee0751e8a83` | test: implement pyghidra vs fission decompiler comparison script and fix hidden cli output |
| 2026-03-04 | `0d85ae25b0fc` | fix(clippy): suppress 36 pedantic lints in fission-loader crate |
| 2026-03-04 | `7d6e5c1169f4` | fix(clippy): resolve 3 clippy errors across fission-disasm and fission-analysis |
| 2026-03-04 | `fec4f59b0da6` | fix(core): resolve 5 clippy errors in fission-core |
| 2026-03-04 | `ce9e2e7fc075` | fix(src): add missing cstring include to 6 source files for Linux GCC compatibility |
| 2026-03-04 | `7f07f15e0c26` | fix(headers): add missing cstdint include to 11 headers for Linux GCC compatibility |
| 2026-03-04 | `ba327559da90` | fix(types): add missing cstdint include to PrototypeEnforcer.h |
| 2026-03-04 | `58ed1142c259` | feat(decompiler): implement Phase 1-4 Ghidra gap mitigations |
| 2026-03-04 | `7f9f9d4d3dbc` | feat: enable Ghidra analysis flags and fix NoReturnDetector API |
| 2026-03-04 | `35a159287c29` | feat: rewrite_pointer_arithmetic_to_array — pointer deref → array subscript |
| 2026-03-03 | `deac984c175a` | feat: implement 8-gap decompiler improvements vs Ghidra |
| 2026-03-03 | `ca470bc99a49` | fix: resolve Dependabot security alerts |
| 2026-03-03 | `e2e33294d83c` | fix(decompiler): include cstdint for uint64_t in StructureAnalyzer header |
| 2026-03-03 | `7408a172aa66` | security: baseline no-fix advisories and restore blocking rust advisory gate |
| 2026-03-03 | `d12316d8c695` | fix: restore switch reconstruction for generic assignment targets |
| 2026-03-03 | `5389762c239a` | refactor: Phase 2.6 - Remove remaining unwraps in pcode optimizer tests |
| 2026-03-03 | `1495b4788297` | refactor: Phase 2.5 - Mutex & path handling safety |
| 2026-03-03 | `e9e62bbbe6db` | refactor: Phase 2.4 - Additional unwrap removal (CLI, loader, pcode, sigs) |
| 2026-03-03 | `719050e91c22` | refactor: Constants library and initial stability improvements |
| 2026-03-01 | `51bfd8df7b82` | fix: Phase A/B optimizer pass safety guards |
| 2026-03-01 | `91c6611dcd66` | fix: decompiler 4 critical bugs + v4 benchmark system (ARM64 69.4%->88.9%, Linux 91.6%) |
| 2026-02-26 | `b02eae287b67` | fix: move strip_shadow_only_params to Step 11.5; fix brace detection |
| 2026-02-26 | `de0a65f4fd02` | fix: IAT indirect call cleanup + shadow param stripping in C++ pipeline |
| 2026-02-26 | `ca6890a23c75` | feat: struct field, IAT calls, shadow params, compound assign normalization |
| 2026-02-25 | `9b2db76f279d` | feat: improve normalize_for_similarity — debug param names, type normalization |
| 2026-02-25 | `a193ed27a7f9` | analysis: improve -O2 decompiler quality (x64 46%→57.9%, x86 49.8%→62.6%) |
| 2026-02-25 | `5e453e9bba37` | x86 double synthesis + normalization fixes: x86 90.1% → 92.6% |
| 2026-02-25 | `be19a1dc583d` | improvement: Track 2/3/4 + benchmark normalization (x86 80% → 90.1%) |
| 2026-02-24 | `580c94316ad2` | chore: track samples/README.md and fix .gitignore |
| 2026-02-24 | `0e0d142f9052` | chore: remove orphaned tests/ directory |
| 2026-02-24 | `9a05960733b7` | Track B: MinGW x86 binary + x86 benchmark suite (baseline 80.0%) |
| 2026-02-24 | `32f7fca96f2b` | Track A+B: normalizer A-1~A-6 patches + TypePropagator mingw branch → 98.8% |
| 2026-02-24 | `b838079ea564` | feat: propagateAcrossReturns + normalizer integer-cast stripping (92.1%) |
| 2026-02-24 | `0f47dc0eb73d` | feat: type propagation guards + normalizer opaque-ptr + FuncCallSpecs fallback |
| 2026-02-24 | `06397c76f16e` | fix: auto-copy libdecomp.dylib to target/debug on macOS in fission-ffi/build.rs |
| 2026-02-24 | `3c47cad53f82` | perf/refactor: Group 1-3 improvements (hash fix, static regex, 2-barrier, string cache, O(n) CFG, lazy arch) |
| 2026-02-24 | `13459511e5f3` | fix: Phase 3 connectivity - batch type propagation + PcodeOptimizationBridge |
| 2026-02-24 | `435c78e072b9` | refactor: Phase 3 decompiler improvements (A-1~D-1) |
| 2026-02-24 | `2eb1e1966f1f` | refactor: Phase 2 decompiler refactoring (A-1~B-4, C-2, D-1, E-1~E-2) |
| 2026-02-24 | `b38ef4d317a4` | arch: fix x86/PE hardcoding in decompiler engine (7 fixes) |
| 2026-02-24 | `9b9dc2107043` | feat(bench): add compare_decompilers_v3.py + suite_example.yaml |
| 2026-02-23 | `0298f08c4ebc` | feat(decompiler): implement TypeSharing, FID→Ghidra feedback, PcodeBridge stability |
| 2026-02-23 | `2deca1acb6e5` | style: split App.css (3386 lines) into src/styles/ zone structure |
| 2026-02-23 | `950836e5c5f2` | refactor(fission-tauri): Phase 2+3 frontend refactoring |
| 2026-02-23 | `086becf25f14` | feat: Tauri GUI Phase 1-9 완전 이관 + egui 제거 |
| 2026-02-21 | `6efbf9a8d4d0` | Phase 10: Exports/Patches/Notes tabs, search sidebar, hex nav, clear fix |
| 2026-02-21 | `0a8672d30342` | feat(tauri-gui): Phase 1-6 완전 구현 + 버그 수정 4건 |
| 2026-02-16 | `e7d3d0735b20` | feat(decomp): expand native feature controls and add quality workflow gate |
| 2026-02-16 | `214c54e365cd` | fix(loader): update tests for DataBuffer and LoadedBinary refactor |
| 2026-01-20 | `936ae9f0f7da` | feat: Enhanced type analysis for Go/DWARF and Swift accessor recognition |
| 2026-01-19 | `09810a839f88` | feat: Swift type recovery and metadata parsing infrastructure |
| 2026-01-19 | `7acf289f6567` | fix: resolve DecompilerNative thread warning spam with per-instance tracking |
| 2026-01-11 | `48287c0bf7a6` | feat(debug): enhance RR/TTD integration with structured GDB/MI parsing and register sync |
| 2026-01-11 | `b2f199621d84` | feat(debug): complete RR/TTD integration and fix macOS dylib loading |
| 2026-01-11 | `dc3ff9079b26` | feat(debug): Add RR Time Travel Debugging integration |
| 2026-01-10 | `12a94865cb9c` | feat: Set native_decomp as default feature |
| 2026-01-10 | `3ea8b2ea2b8f` | feat: Decompiler error handling & recovery improvements |
| 2026-01-10 | `704a7fdc3670` | perf: LoadedBinary cloning optimization with Arc<Vec<u8>> |
| 2026-01-10 | `af733aea4bba` | refactor: GUI Architecture & Native Decompiler Stabilization |
| 2026-01-10 | `d99ba204bf9b` | feat(cfg): Add CFG analysis integration for CLI and GUI |
| 2026-01-09 | `7fd05ca751b7` | feat: Improve error handling and reorganize GUI module structure (2026-01-09) |
| 2026-01-09 | `e022b976c904` | fix: Correct function address extraction for PE binaries |
| 2026-01-08 | `40d0e5883c88` | test: Add comprehensive test suite for complex decompilation patterns |
| 2026-01-08 | `3f656f352609` | feat: Achieve 97.86% decompiler similarity with Ghidra |
| 2026-01-05 | `7332a9a129db` | feat: Implement COFF Symbol Table parsing for 100% MinGW function recognition |
| 2026-01-05 | `70234f3761aa` | feat: Add Mach-O symbol resolution and decompiler comparison framework |
| 2026-01-05 | `cf32139d0660` | feat: Add Pcode graph visualization with assembly integration |
| 2026-01-05 | `6a2365f40132` | docs: Reorganize changelog by topic instead of dates |
