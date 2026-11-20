# FerriSync Python security notes

This document describes security-relevant behavior and operational guidance for
the Python bindings (`ferrisync` / `ferrisync-python`).

## Security model (inherited)

The Python package is a PyO3/maturin frontend over `ferrisync-core`. It reuses
the same:

- TLS 1.3 transport and mutual authentication
- TOFU (Trust On First Use) certificate pinning
- device identity (`identity.crt`, `identity.key`) and pairing metadata
  (`metadata.db`) in the FerriSync data directory
- sync, conflict, and path-safety logic implemented in the Rust core

Python code does not reimplement cryptography, pairing, or sync decisions.

## Identity and data directory

- Use `Client(data_dir=...)` / `AsyncClient(data_dir=...)` to select the
  FerriSync identity and state to use.
- Multiple frontends (CLI, Flutter, Python) can share the same `data_dir`
  sequentially to reuse identity and pairing state.
- Use separate `data_dir` values for separate logical devices/identities.
- Do not copy private key material (`identity.key`) to untrusted systems.

## Pairing and trust

- Pairing establishes trust with a peer identity (certificate-derived device id).
- Unknown peers should require explicit operator consent.
- `serve(..., auto_accept=True)` admits unknown peers automatically and grants
  read/write access to the served folder for any device that can reach the
  host. **Use only on trusted private networks.** Prefer the default confirm
  policy (`auto_accept=False`) and approve/deny via
  `pending_pairings()` / `approve_pairing()` / `deny_pairing()`.
- A TOFU mismatch must fail closed. Do not bypass verification or silently
  replace trusted identities.

## File access and path safety

- Sync operates within configured folder roots managed by the core engine.
- Path traversal and unsafe path handling are enforced by core path-safety
  checks; callers should still pass valid local directories and handle errors.
- Synced file content comes from peers. FerriSync does not scan for malware or
  validate that received file contents are safe to execute/open.

## Threat model limitations

Current limitations (shared with FerriSync overall):

- Assumes the local device and operating system are trusted.
- A compromised paired device can send malicious or unwanted file content.
- No protection against other local software reading synced data.
- No malware/content scanning of received files.

## Logging and error handling

- Avoid logging secrets, private keys, or sensitive file contents.
- Treat error messages as potentially useful for diagnostics but do not assume
  they are safe to expose publicly without review.
- Handle `FerriSyncError` subclasses explicitly where policy decisions depend on
  failure type (e.g. closed client, invalid input, not found, cancellation).

## Cancellation and concurrency

- Cancellation is cooperative. Cancelled operations may continue briefly in
  native code; discard results from cancelled operations.
- `AsyncClient` uses a bounded executor; do not assume unlimited parallelism.
- Closing a client stops owned servers/watches and shuts down async executor
  resources, but in-progress native work may take time to unwind.

## Recommendations

- Run FerriSync/Python automation on trusted hosts and networks.
- Pin and review dependencies in production environments.
- Use explicit `data_dir` paths and least-privilege filesystem permissions.
- Prefer confirmation-based serving and explicit pairing approval.
- Validate and sandbox any automated processing of synced files.
