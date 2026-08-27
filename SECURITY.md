# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.1.x   | Yes       |

## Reporting a Vulnerability

If you discover a security vulnerability in FerriSync, please report it
responsibly. **Do not open a public GitHub issue.**

Email: [create an issue labeled "security" on GitHub](https://github.com/DavidNeurieder/FerriSync/issues)
with a description of the vulnerability. We will respond within 7 days.

## Security Model

FerriSync provides peer-to-peer file synchronization with the following
security properties:

### Transport Security

- **TLS 1.3 on every connection** — all data in transit is encrypted and
  authenticated using mutually-authenticated TLS
- **Certificate-based identity** — each device generates a self-signed
  certificate; the certificate fingerprint serves as the device's unique identity

### Authentication

- **Trust On First Use (TOFU)** — the first time two devices connect, the
  operator must explicitly approve the pairing
- **Certificate pinning** — once paired, the peer's certificate fingerprint is
  pinned in the local database; connections from the same device identity with
  a different certificate are rejected
- **Pairing consent** — unknown devices are held for operator approval before
  they can sync

### Data Integrity

- **BLAKE3 hashing** — every file transfer includes a content hash; the
  receiver verifies the hash before committing the file to disk
- **Atomic writes** — received files are written to a temporary file and
  atomically renamed into place, preventing partial writes on crash
- **Conflict preservation** — when two devices modify the same file, the
  overwritten version is preserved as a backup (`.ferrisync-conflict-*`)

### Input Validation

- **Frame size limits** — protocol messages are bounded (4 MiB control frames,
  1 MiB data frames)
- **Index entry limits** — remote indexes are capped at 100,000 entries to
  prevent memory exhaustion
- **Path length limits** — file paths are validated against a 4 KiB maximum
- **Path traversal protection** — `..` components, null bytes, and absolute
  paths are rejected; all file operations go through a safe-join check

### Known Limitations

FerriSync currently **assumes the local device and operating system are
trusted**. It does not protect against:

- A paired device that is itself compromised
- Malicious file *content* received from a trusted peer (no malware scanning)
- Other software running on the same machine reading synced data
- Simultaneous edits by multiple devices (last-writer-wins conflict resolution;
  no three-way merge)

## Platform Support

FerriSync is tested on:

- **Linux** (x86_64) — primary development platform
- **Android** (arm64, x86_64) — via Flutter client
- **macOS** — should work, not regularly tested
- **Windows** — should work, not regularly tested

The sync protocol is platform-independent; TLS certificates and file hashes
are portable across platforms.

## Cryptographic Primitives

| Primitive | Implementation |
|-----------|---------------|
| TLS       | rustls 0.23 (ring backend) |
| Hashing   | BLAKE3 |
| X.509     | rcgen + x509-parser |
| Certificates | Self-signed, Ed25519 keys |
