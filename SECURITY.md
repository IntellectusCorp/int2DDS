# Security Policy

## Supported Versions

int2DDS is currently in early development (`0.0.x`). Security fixes are
applied to the latest released version on the `main` branch only.

| Version | Supported          |
| ------- | ------------------ |
| 0.0.x   | :white_check_mark: |
| < 0.0.1 | :x:                |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues,
pull requests, or discussions.**

If you believe you have found a security vulnerability in int2DDS, report it
privately by email:

📧 **int2dds@int2.us**

To help us triage and resolve the issue quickly, please include as much of the
following as you can:

- Type of issue (e.g. buffer overflow, RTPS packet parsing flaw, denial of
  service, memory safety, information disclosure)
- Affected component (`dds`, `derive`, `ffi`, `rpc`, `idl`, or a language binding)
- Affected version or commit hash
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code, if available
- Impact of the issue and how an attacker might exploit it

You may encrypt sensitive details or request a secure channel in your initial
email.

## Response Process

- **Acknowledgement** — We aim to acknowledge your report within **5 business
  days**.
- **Assessment** — We will investigate, confirm the issue, and determine the
  affected versions, keeping you informed of our progress.
- **Fix & disclosure** — We will work on a fix and coordinate a disclosure
  timeline with you. We follow a coordinated (responsible) disclosure model and
  ask that you keep the report confidential until a fix is released.
- **Credit** — With your permission, we are happy to credit you for the
  discovery once the issue is resolved.

Thank you for helping keep int2DDS and its users safe.
