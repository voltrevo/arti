# subtle-tls known issues

## Will cause connection failures

### ECDSA signature conversion uses hash to guess curve size
`cert.rs:711-713` — Coordinate size for DER-to-raw conversion is inferred from
the hash algorithm (SHA-256 → 32, SHA-384 → 48) instead of the actual curve
from the public key. A cert using P-384 with SHA-256 will produce a 64-byte
raw signature when SubtleCrypto expects 96 bytes, causing verification failure.
The correct curve is already extracted in `get_crypto_algorithm_from_key`; the
coord size should be derived from it and threaded through to the conversion call.

### RSA-PSS hash hardcoded to SHA-256
`cert.rs:478-490` — The RSA-PSS branch always uses SHA-256 instead of parsing
the hash from the AlgorithmIdentifier parameters. Certificates signed with
RSA-PSS using SHA-384 or SHA-512 will fail signature verification.

### IP address SAN matching is broken
`cert.rs:118-126` — IP addresses in X.509 SANs are raw bytes (4 for IPv4, 16
for IPv6), not UTF-8 strings. `std::str::from_utf8(ip_bytes)` will almost never
succeed. Not a problem for Tor (hostname-based) but broken for direct-IP
connections.

## Security gaps (accepts things it shouldn't)

### No basicConstraints validation
`cert.rs` `verify_chain_signatures` — Intermediate certificates are not checked
for `basicConstraints: CA=TRUE`. A leaf certificate could be presented as an
intermediate to sign arbitrary certificates. `pathLenConstraint` is also not
enforced.

### No keyUsage / extendedKeyUsage checks
`cert.rs` `verify_chain_signatures` — No validation that intermediates have
`keyCertSign` set or that leaf certificates include `id-kp-serverAuth`. Accepts
certificates not intended for TLS server authentication.
