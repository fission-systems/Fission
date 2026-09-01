
\# Fission — handoff



Written 2026-08-26. Tree is clean, everything below is pushed; \`main\` is at

\`6830a7f47\`. Nothing is half-applied — pick up from the "Open work" section.



\---



\## 1. What this session was doing



Two threads, in this order:



1. \*\*Score Fission against the real DecBench corpus locally\*\*, so changes can

   be measured instead of guessed at.
2. \*\*Fix what that measurement found\*\* — a series of correctness, memory and

   time defects, all found by running the real corpus rather than by reading

   code.



Thread 1 is set up and working. Thread 2 is partly done; the remaining items

are in "Open work".



\---



\## 2. The local benchmark harness (working — do not rebuild it)



\### Dataset



\`\~/fission-benchmark/decbench-data/\` holds the HuggingFace dataset

(\`noelo-lab/decbench-dataset\`), fetched with plain \`curl\` against

\`[https://huggingface.co/datasets/noelo-lab/decbench-dataset/resolve/main/](https://huggingface.co/datasets/noelo-lab/decbench-dataset/resolve/main/)\<path>\`.

\`huggingface\_hub\` is \*\*not\*\* installed and is not needed. Every artifact path

is listed in \`manifest\_unoptimized.json\`.



- 267 binaries, 34,406 functions, 266 with published source CFGs.
- \*\*The dataset binaries carry DWARF and symtab\*\* (unlike the evalkit ones,

  which have neither) — this is why Types is scoreable locally at all.
- \`scores\_unoptimized.json\` (114MB, \`configs/unoptimized/function\_results.json\`)

  carries \*\*per-function published scores for all 13 tools\*\*, so any local

  change can be compared against the leaderboard on exactly its own rows.



Some of these binaries are compiled-from-source malware (\`mirai\`, \`mydoom\`,

\`dexter\`). \*\*Static analysis only — never execute them.\*\* Decompiling is fine.



\### Driving the CLI



\`fission\_cli decomp\` has \*\*no \`--function\` flag\*\*; use \`--addresses-file\` with

addresses from \`llvm-nm\`:


```perl
llvm-nm --defined-only "$BIN" | awk '$2=="t"||$2=="T"{print "0x"$1}' > addrs.txt
fission_cli decomp "$BIN" --layer nir --json --no-header --no-warnings \
  --addresses-file addrs.txt
```



Per-function timings come back in the JSON's \`preview\_build\_stats\` as

\`build\_duration\_ms\` / \`structuring\_duration\_ms\` / \`normalize\_duration\_ms\` —

use these to find slow functions instead of bisecting.



\### byte\_match needs a Docker shim



This Mac is arm64, so DecBench asks for \`x86\_64-linux-gnu-gcc\` for the x86

corpus, does not find it, and \*\*abstains on the whole corpus\*\*. Rather than

install a cross toolchain, a shim at \`\~/.local/fission-xcc/x86\_64-linux-gnu-gcc\`

forwards the compile into a running \`linux/amd64\` container:


```javascript
docker run -d --name fission-x86gcc --platform linux/amd64 \
  -v "$SCRATCH/xcc-work:/work" gcc:13 sleep infinity
export FISSION_XCC_WORK="$SCRATCH/xcc-work"
export PATH="$HOME/.local/fission-xcc:$PATH"
export DECBENCH_NO_CACHE=1
```



Three things that cost time to discover, all required:



- \*\*\`gcc:13\`, not \`gcc:14\`.\*\* GCC 14 promotes \`-Wint-conversion\` to an error;

  DecBench's own image uses 13.
- \*\*\`context\_decls\`.\*\* \`byte\_match\` derives sibling prototypes from \*every\*

  function in the binary (the real adapter runs \`--all\`). Scoring a subset

  without that fails to compile on the first call to a sibling.
- \*\*\`DECBENCH\_NO\_CACHE=1\`.\*\* The metric cache key does not include the gcc

  version, so a failure cached under gcc:14 keeps replaying.



\### Scripts



The scratchpad is \`/private/tmp/claude-501/.../scratchpad\` and \*\*is wiped on

reboot\*\* — the machine was restarted once this session and everything in it

was lost. Rewrite these into the repo if they are worth keeping:



- \`fullrun.py\` — scores GED + Types + byte\_match over the whole manifest,

  resumable, chunked.
- \`memscan.py\` — peak-memory census, decompile-only, with a watchdog that

  kills a child past \`MEM\_CAP\_MB\` and records what it was decompiling.
- \`sweep.py\` — per-function peak RSS for one binary.
- \`bisect\_mem.py\` — halves a memory-capped chunk to isolate one function.



\### \*\*Read this before running anything big\*\*



The first full-corpus attempt \*\*took the machine down\*\* — memory went past

100GB and it had to be force-restarted. Two mistakes, both mine:



1. pyjoern costs \~450MB baseline \*\*plus \~10MB per function\*\* in one

   \`extract\_decompiled\_cfgs\` call. \`bash\`'s 2,881 functions is a \~29GB single

   call. Chunk it (50 functions/call).
2. The second attempt chunked joern but not fission, and \`bzip2\` alone peaked

   at 3.9GB. Chunk the decompile too (300 addresses/call), and watch RSS.



Validate a memory bound on a \*\*worst case\*\* (\`bzip2\`, \`bash\`), not on

\`coreutils/ls\`. Watch swap: it grew 2GB per 40 seconds before the near-miss.



\---



\## 3. Where Fission actually stands



\### The leaderboard number is not what it looks like



On \`unoptimized\` (34,406 functions) Fission was scored on \*\*100\*\*. So were

codex, claude-code and glaurung; manifold got 34. ida/angr/kuna were scored on

32,000+. The leaderboard's shared denominator of 34,406 is why Fission shows

0.1%.



Recomputed on Fission's own 100 rows, Union (perfect on ≥1 metric):



\| tool | GED | Types | byte | \*\*Union\*\* |

\|---|---|---|---|---|

\| codex | 72.7% | 20.2% | 47.4% | \*\*79.0%\*\* |

\| claude-code | 69.7% | 16.9% | 28.9% | \*\*72.0%\*\* |

\| angr | 41.4% | 16.5% | 1.2% | \*\*52.9%\*\* |

\| binja | 42.7% | 22.7% | 0.0% | \*\*52.4%\*\* |

\| ida | 42.6% | 11.6% | 1.1% | \*\*50.0%\*\* |

\| glaurung | 40.0% | 18.0% | 7.7% | \*\*49.0%\*\* |

\| kuna | 42.6% | 10.5% | 3.2% | \*\*46.8%\*\* |

\| ghidra | 33.0% | 9.4% | 0.0% | \*\*36.2%\*\* |

\| \*\*fission\*\* | \*\*31.0%\*\* | \*\*9.0%\*\* | \*\*5.1%\*\* | \*\*36.0%\*\* |

\| r2dec | 36.9% | 1.3% | 0.0% | 35.2% |



\*\*9th of 13, tied with Ghidra.\*\* Restricting to the 69 rows every static tool

scored gives the same ordering, so this is not survivor bias. The published

row is version \*\*0.2.1\*\*.



Quote this table, never the raw leaderboard percentages, when comparing

against ida/angr/kuna.



\### This session's measured improvement



Reproducing those 100 rows locally (88 of them reproduce; 7 lack symbols,

1 fails to decompile), against published 0.2.1:



\|  | published | now |

\|---|---|---|

\| GED perfect | 30 | \*\*33\*\* |

\| mean GED | 27.4 | \*\*16.6\*\* |



The harness agrees with the leaderboard: exact GED match on 63–65 of 88, and

perfect/not-perfect agreement on 82–84 of 88.



\### Two facts worth keeping



- \*\*byte\_match is where the whole field is weak\*\* — on the x86 rows ghidra

  scores 0/39 perfect, ida 0/39, fission 2/39. Only the LLMs do well (codex

  47%). If "functionally equivalent" is the project's thesis, this is the

  metric that measures it, and it is wide open.
- \*\*ARM byte\_match is a dead zone for everyone.\*\* Fission's 61 unscored

  byte\_match rows are all ARM firmware; on those same rows ghidra averages

  0.002 and kuna 0.000. The gap is environmental (the scoring machine had no

  \`arm-none-eabi-gcc\`) and worth \~nothing in score. Report it to the

  maintainer as a data-quality note, do not chase it.



\---



\## 4. What was fixed (13 commits, all pushed)



Read the commit messages — each carries its own measurement.



\*\*Correctness\*\*



- \`0898c5bc2\` a call defines the ABI result register. SLEIGH's \`Call\` declares

  no output, so a read of the result register after a call resolved to the

  last definition \*before\* it — at \`-O0\` that is an argument staged through

  the same register. \`stat(path,&st) < 0\` came out as \`if (\_\_buf < 0)\`.
- \`369cf1653\` read a stack displacement as signed. A 32-bit \`lea ebp,-168\`

  stored as \`0xffffff58\` became parameter index 1,073,741,780, and every slot

  up to it got a \`param\_N\` binding: four \`dexter.dll\` functions each hit 3GB

  in two seconds.
- \`c8fa6313a\` subtract a constant that is negative at its own width

  (\`esp + 4294967128\` → \`esp - 168\`). \*\*Note the bug I introduced and fixed

  here\*\*: at the exact midpoint of the width, \`span - value\` is the same

  value, so \`Add\`/\`Sub\` flip forever. Same shape as the \`c != i64::MIN\` guard

  the old path already had.
- \`b739357d6\` \*\*output was nondeterministic\*\*. Two file-scope statics can

  share a name; the emitted C declares one, and the winner came from

  \`HashMap\<u64,String>\` iteration order. Six \`gzip\` functions alternated

  between \`ushort bitbuf\` and \`ulonglong bitbuf\` across runs of one unchanged

  binary. Now chosen in address order.



\*\*Structure (GED)\*\*



- \`ef316acf2\` + \`b64dc4f26\` end the path at a non-returning call, and unwrap

  the \`else\` behind such an arm — but \*\*only\*\* when the arm leaves by a call,

  never by \`return\`. The first version fixed \`copyFileName\` and broke

  \`user\_name\`; the 100-row reproduction caught it.
- \`188351dfc\` hoist a cast both arms of a conditional share. pyjoern gives a

  conditional whose arms carry casts its own branch (4 nodes) and a plain one

  a single node. \`gzip/get\_method\` GED 707 → 182.



\*\*Memory / time\*\*



- \`4442e0f50\` bound the reaching condition a branchy region builds. Growth is

  in the number of \*paths\*: \`bzip2\`'s \`main\` reached 8,729,059 expression

  nodes and 9.2GB. Now 0.54GB.
- \`411d5b790\` bound total work in one varnode lowering. \`visiting\` is

  path-scoped, so a value reachable by many paths is lowered once per path:

  \`coreutils/fmt\`'s \`put\_line\`, 4 blocks and 201 bytes, entered

  \`lower\_varnode\_inner\` fourteen million times.
- \`6830a7f47\` bound the \*climb\*, not just the meet, in the idom walk. Cooper's

  termination needs RPO to decrease up the chain, which needs every node

  reachable; a truncated CFG breaks it and a climb walks a 2-cycle forever.

  \`coreutils/test\`/\`expr\`/\`[\` 900s → 3s.
- \`681b91bbb\`, \`531df3b32\` guard-term copying and two per-block caches.

  \`bzip2\` 60.3s → 33.0s.



Net on the three x86 binaries used throughout: \`bzip2\` 60.3s→29s /

9.24GB→0.54GB, \`gzip\` 24.8s→6.6s / 10.36GB→0.32GB.



\---



\## 5. Open work



\### A. \`diff/do\_printf\_spec\` — cause found, fix not written



Hangs (>300s). It is the \*\*\`stable-prefix\` re-decode\*\* in the callee-summary

path (\`crates/fission-decompiler/src/facts/facts.rs\`): the callee is decoded a

second time at \`max\_bytes/4\` instructions to harvest extra pointer evidence,

which produces a \*\*truncated CFG\*\*, and something downstream of that is

pathological on truncated graphs.



Measured, by gating each piece off:



- prototype inference entirely off: 60s → 2.04s
- \*\*stable-prefix only off: 60s → 2.92s\*\*



\`6830a7f47\` fixed one unbounded loop reachable this way; there is at least one

more. Gating the feature off is not the fix — bound the path, or stop feeding

structuring a truncated graph.



\### B. \`sort/merge\` — separate cause, not identified



Still hangs with prototype inference off, so it is \*\*not\*\* the same as A.

What has been ruled out, with numbers:



- \`try\_alternative\_structurings\`: 45 candidates, \`normalized()\` totals 0.06s.
- expression size: 1–2 nodes.
- the 22-rule loop in \`normalize\_expr\`: under 200 rounds.



What is established: \`normalize\_expr\` is entered \*\*\~80 million times\*\*,

concentrated in one \`normalize\_hir\_function\` call on a body with 52 statements

and \*\*417 locals\*\*, and the last pass to complete is

\`loop\_condition\_temp\_inline\_late\`. It hangs inside a callee summary for

\`avoid\_trashing\_input\`, which decompiles directly in 1.75s.



The 417 locals on a 22-block function is the suspicious number. Start there.



\### C. The memory census is unfinished



\`memscan.py\` got through \*\*136 of 267 binaries\*\* before being stopped, and

that was before five of the fixes above landed, so its numbers are stale.

Binaries it flagged, minus the ones now fixed:



- still open: \`chibios/ch\`, \`betaflight\`, \`cleanflight\`, \`crazyflie/cf2\`,

  \`bash/bash\`, \`coreutils/sort\`
- fixed since: \`dexter\`, \`coreutils/fmt\`, \`coreutils/[\`, \`expr\`, \`test\`
- \`diffutils/diff\` — memory fixed (10.8GB → 0.67GB), still slow (item A)



Re-run it before trusting any of it.



\### D. Then: the full-corpus run



The point of all of the above. With the memory defects fixed a full run is no

longer a threat to the machine, but re-validate the bound first. Decompile

alone was \~2 hours single-stream before these fixes; the median function is

0.03s and the mean 0.20s, so the tail is most of it.



\*\*Why it is worth doing\*\*: Zion's own answer on overfitting is that the full

benchmark is the guardrail — "if it overfits for a sample, I see it lose

points across the entire benchmark." A 100-row sample is a weak guardrail, and

this session has an example: one commit fixed the function it targeted and

broke another, and mean GED barely moved (16.59 → 16.64).



\---



\## 6. Things that were tried and rejected — do not redo these



Each was measured, not judged.



- \*\*\`Box\` → \`Rc\` for \`PreHirExpr\`.\*\* Expression cloning is \~28% of builder

  time, so this looks obvious. It is not: the code mutates trees in place

  through \`&mut\` and \*\*never shares\*\* — 1,829 of 1,829 \`Rc::make\_mut\` calls on

  statement bodies had \`strong\_count == 1\`. Rc would add refcount cost and

  enable nothing until the passes are rewritten to share. 63 compile errors in

  one crate; 166 files mention the type.
- \*\*Caching callee summaries\*\* by \`(binary hash, address)\`. Works, saves \~15%,

  and \*\*makes output nondeterministic\*\*: the summary depends on the caller's

  \`NirTypeContext\`, and the batch is parallel, so which caller fills the cache

  first decides the answer. Six \`gzip\` functions changed per run. If you want

  this, first make the callee prototype caller-independent — that is a

  semantic change needing its own evaluation.
- \*\*Memoizing \`AbiState::register\_namer\`\*\* (53 call sites, rebuilds the whole

  register model per call): no change. The hot path goes through

  \`PreviewBuilder::register\_namer\`, which was already memoized.
- \*\*Caching \`varnode\_aliases\_value\`\*\* by key pair: 32.68s vs 33.03s, inside

  the run-to-run spread. Hashing two \`VarnodeKey\`s costs about what the four

  predicates cost.
- \*\*Caching \`current\_join\_register\_update\_reads\_live\_register\`\*\*: no change.



\---



\## 7. How to not waste time (things that bit me)



- \*\*The profiler misattributes across inlined frames.\*\* It named

  \`ImmPostDomTree::compute\` and \`try\_alternative\_structurings\` as hotspots

  three separate times; both were neighbours of the real cost. Confirm with a

  counter before acting on a profile.
- \*\*Instrument at entry, not exit.\*\* Every print placed after a call returns

  says nothing about the call that never returns. This cost several rounds on

  the \`coreutils/test\` hang.
- \*\*A background \`cargo build\` moves a 33s benchmark by 3 seconds\*\* — larger

  than most effects being measured. Take medians of three runs after the

  machine settles. One "improvement" and one "regression" this session were

  both build noise.
- \*\*Rebuild after \`git stash pop\`.\*\* A "gzip is unaffected" reading was taken

  against a stale binary and sent me down a false trail.
- \*\*\`git checkout --\` deletes uncommitted work.\*\* It ate a finished

  implementation once; it had to be retyped.
- \*\*Output was nondeterministic until \`b739357d6\`.\*\* Every "output unchanged"

  check before that commit compared two runs of a decompiler that could

  disagree with itself. Re-verify anything that rested on one.



\---



\## 8. Standing constraints



- Push to \`main\` only. \*\*Do not cut release tags\*\* unless explicitly asked.
- Do not delete \`vendor/decbench-evalkit/.../binaries/\<name>\_ghidra/\` caches.
- The corpus includes real malware — static analysis only.
- \`fission-emulator\` has 7 pre-existing failures; \`main\`'s own CI shows the

  same set. \`fission-dir::corpus\_decompilations\_match\_real\_machine\_code\` also

  times out at \`origin/main\` — CI skips it because the corpus lives outside

  the repo, so it only appears locally. Neither is yours.

