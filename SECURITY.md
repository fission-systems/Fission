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

## Samples and attachments

Do **not** attach malware or unsolicited exploit binaries to issues or pull requests.

Preferred evidence:

- **SHA256 hashes** of benign reproduction fixtures already in-repo (`benchmark/binary/`)
- References to **publicly documented** benign corpora
- For sensitive benign fixtures, **password-protected archives** arranged out-of-band after maintainer ACK

Operational expectations for CI fixtures: [`docs/MALWARE_SAMPLE_POLICY.md`](docs/MALWARE_SAMPLE_POLICY.md).

## Scope notes

Third-party vendored trees ([`THIRD_PARTY.md`](THIRD_PARTY.md)) inherit upstream policies; report critical issues upstream when they originate there.
