# Fuzz Testing for subtle-tls

This directory contains fuzz targets for testing the TLS parsing code.

## Prerequisites

Install cargo-fuzz (requires nightly Rust):

```bash
rustup install nightly
cargo +nightly install cargo-fuzz
```

## Running Fuzz Tests

```bash
cd subtle-tls/fuzz

# Run a specific fuzz target
cargo +nightly fuzz run fuzz_certificate

# Run with a time limit (e.g., 60 seconds)
cargo +nightly fuzz run fuzz_certificate -- -max_total_time=60

# Run all fuzz targets
for target in fuzz_server_hello fuzz_certificate fuzz_record fuzz_handshake_parse; do
    cargo +nightly fuzz run $target -- -max_total_time=30
done
```

## Fuzz Targets

| Target | Description |
|--------|-------------|
| `fuzz_server_hello` | Tests handshake header, Finished, and CertificateVerify parsing |
| `fuzz_certificate` | Tests TLS 1.3 and 1.2 certificate chain parsing |
| `fuzz_record` | Tests TLS record layer bounds checking |
| `fuzz_handshake_parse` | Tests complete handshake message parsing |

## Reproducing Crashes

If a crash is found, it will be saved in `fuzz/artifacts/<target>/`. To reproduce:

```bash
cargo +nightly fuzz run fuzz_certificate fuzz/artifacts/fuzz_certificate/crash-xxx
```

## Coverage

To generate coverage reports:

```bash
cargo +nightly fuzz coverage fuzz_certificate
```
