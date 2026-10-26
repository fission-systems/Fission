# Linux HLE error contracts reviewed on 2026-10-26

Two remaining syscall contracts from the alternative #616/#569 review need explicit coverage:

- Unsupported terminal process-group and window-size setters return ENOTTY.
- prlimit64 rejects a negative PID with ESRCH and leaves the old-limit output buffer unchanged.

Both focused regressions failed before the change and passed afterwards. Existing futex scheduler semantics are preserved. This is a syscall contract correction, with no decompiler quality claim.

The initial reconciliation candidate also imposed a new PE descriptor count policy. That policy was removed: EOF already bounds parsing and a fixed count could reject a valid input. Duplicate PR histories are excluded; their separate closure does not imply additional implementation here.

Validation: focused syscall regressions plus the workspace nextest suite, workspace check/clippy, and formatting. Exact final results are recorded in the PR description.
