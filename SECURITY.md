# Security Policy

## Supported Versions

ManaOS is pre-release software. Security fixes are handled on the active
development branch. There are no long-term support branches or stable release
channels yet.

## Reporting a Vulnerability

Please report suspected vulnerabilities privately to the maintainers before
opening a public issue.

Include:

- affected commit or branch
- reproduction steps
- expected impact
- any QEMU or serial logs
- local configuration changes, if any

Do not include exploit code beyond what is needed to reproduce the issue.

## Security-Sensitive Areas

ManaOS is a kernel, so bugs that would be ordinary application defects can cross
privilege and memory boundaries. Treat the following as security-sensitive even
when exploitability is not yet proven:

- user pointer validation bypasses
- incorrect kernel/user page permissions
- writable executable mappings
- missing syscall argument validation
- interrupt, exception, or syscall paths that allocate or take unsafe locks
- physical frame double frees, owner mismatches, or use-after-free paths
- storage, FAT32, GPT, or ELF parser bounds-checking bugs
- DMA ownership mistakes that let a device access reused frames
- diagnostics that expose sensitive kernel addresses in release-like builds

When in doubt, report privately first. A minimal reproducer, the failing serial
log, and the expected versus actual behavior are more useful than a large patch.

## Maintainer Handling Notes

Maintainers should keep the initial report private until the impact is
understood. Prefer a focused fix branch, local verification, and a concise public
summary after the fix is merged. For boot-visible security fixes, include
`just storage-smoke` evidence or explain why the smoke path does not cover the
affected behavior.
