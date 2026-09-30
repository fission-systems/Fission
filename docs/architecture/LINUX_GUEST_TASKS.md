# Linux guest task model

Fission models a bounded subset of Linux threads in `fission-emulator`. The
model is deterministic and cooperative: all guest tasks execute on the same
host thread and share one address space, VFS, file table, and process-level
HLE state. Each task retains its own CPU registers, TLS base, signal mask and
pending signal state, and `clear_child_tid` address.

## Supported behavior

- `clone` creates a thread only when the required shared-process flags are
  present and the exit-signal byte is zero. The implemented ABI layouts cover
  x86-64 and AArch64. Invalid stacks or TID pointers return `-EINVAL` or
  `-EFAULT`; unsupported flag combinations return `-ENOSYS`.
- `CLONE_PARENT_SETTID`, `CLONE_CHILD_SETTID`, and
  `CLONE_CHILD_CLEARTID` update the supplied guest memory. The clear operation
  also wakes one waiter at that address when the task exits.
- `CLONE_SETTLS` is accepted only when the architecture exposes a modeled
  per-task TLS register; otherwise the clone request returns `-ENOSYS`.
- `FUTEX_WAIT` and `FUTEX_WAKE`, with or without `FUTEX_PRIVATE_FLAG`, use a
  FIFO wait queue per guest address. A value mismatch returns `-EAGAIN`; wake
  returns the number actually resumed, capped by the requested limit. Other
  futex commands return `-ENOSYS`.
- `sched_yield` moves the current task behind other runnable tasks. `gettid`,
  `set_tid_address`, and `tkill` use task-local identities and state. An
  unmasked signal sent to a futex waiter resumes it with `-EINTR`.
- `exit` terminates one task. `exit_group` remains process-wide and ends the
  emulation after any requested exit code is recorded.
- `fork`, `vfork`, `execve`, and `clone3` are not process-modelled and return
  `-ENOSYS`.

## Scheduling and timeouts

The scheduler switches tasks only at a supported `futex`, `sched_yield`, or
thread-exit syscall. `clone` adds the child to the runnable queue but returns
to the parent. A futex wait blocks its caller until a matching wake, an
unmasked `tkill`, or timeout. A zero timeout expires immediately. Positive
timeouts use deterministic virtual time and expire when no task is runnable;
they do not preempt a guest task that keeps running without yielding.

If every remaining task is blocked and no modeled timeout can make progress,
the emulator stops with `guest_deadlock` instead of returning a fabricated
success value. The model does not emulate host threads, preemptive scheduling,
process creation, cross-process shared futexes, robust futex lists, or the
broader Linux signal-delivery ABI.

The libc-free two-task regression fixture is maintained in
`crates/fission-emulator/testdata/src/linux_guest_futex.S`; the checked-in ELF
is executed only inside Fission by `tests/linux_guest_tasks.rs`.
