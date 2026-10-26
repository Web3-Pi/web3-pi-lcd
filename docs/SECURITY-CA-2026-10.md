# TLS trust security update

The locked `webpki-roots` moves from 1.0.6 to 1.0.10 and `rustls-webpki`
from 0.103.9 to 0.103.16. The first removes the distrusted emSign root;
the second rejects certificate paths using the compromised intermediate
public keys, including alternate cross-sign paths.

This remains a bundled Mozilla trust store. Updating Raspberry Pi's
`ca-certificates` alone does not update the binary. Build and distribute
the application after lockfile updates. CI/release checks reject old TLS
versions and builds use `--locked`.

Validation: run `python3 scripts/check-tls-lock.py`, `cargo test --locked`
and `mise run build-aarch64` on Linux ARM64. GPIO dependencies are Linux-only;
macOS is not a supported native test target. Verify a candidate on one Pi
before publishing a fleet release; retain the previous binary for rollback.

Sources:
- https://rustsec.org/advisories/RUSTSEC-2026-0381.html
- https://rustsec.org/advisories/RUSTSEC-2026-0382.html
