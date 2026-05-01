# Security policy

**Last verified:** 2026-05-02

## Supported versions

Security fixes land on **`main`** first and ride the next tagged release ([`docs/VERSIONING.md`](docs/VERSIONING.md)). Older tags are **best-effort** unless explicitly declared as extended-support.

## Reporting a vulnerability

Please **email the repository maintainer** (GitHub profile contact) rather than filing a public issue for exploit-ready bugs.

Include:

- Affected component (`fission-cli`, loader, automation lane, etc.)
- Minimal reproduction steps **without attaching malicious binaries**
- Impact hypothesis (remote vs local, integrity vs confidentiality)

Allow a reasonable coordination window before public disclosure.

## Crash, malformed binary, and parser bugs

Fission parses untrusted binaries **locally**. Not every crash or rejection counts as a security vulnerability:

- **File as a normal bug** (public GitHub issue is fine): deterministic failures on **benign or synthetic** fixtures; incorrect lifted output or automation deltas with **no plausible sandbox escape**; detector misclassification when impact stays analytical-only.
- **Use coordinated disclosure first** (email, not a public issue): suspected memory corruption exploitable beyond fail-fast abort; confidentiality or integrity breaks outside “CLI user analyzes local file”; anything that looks like a viable sandbox/container escape.

If you are unsure, email first—we can triage quickly.

## Samples and attachments

Do **not** attach malware, live offensive samples, or unsolicited exploit binaries to issues or pull requests. Do **not** paste credential-bearing or sensitive binaries publicly.

Preferred evidence:

- **SHA256 hashes** (and format/size metadata) rather than raw bytes when possible
- References to **benign** reproduction fixtures already in-repo (`benchmark/binary/`) or **publicly documented** benign corpora
- **Password-protected archives** or private links **only after maintainer acknowledgement**, never dropped unsolicited into threads

Operational expectations for CI fixtures and escalation: [`docs/MALWARE_SAMPLE_POLICY.md`](docs/MALWARE_SAMPLE_POLICY.md).

## Scope notes

Third-party vendored trees ([`THIRD_PARTY.md`](THIRD_PARTY.md)) inherit upstream policies; report critical issues upstream when they originate there.
