# Review Log

Issues found and resolved during code review of the WASM changes.

## Resolved

1. **`tor-proto/src/client/reactor/circuit.rs`** — Unnecessary `.collect()` into Vec before `.any()`. Reverted to upstream's direct `.any()` on iterator. Was leftover from removed debug counters.

2. **`tor-dirclient/src/lib.rs`** — Read timeout changed from total to per-read idle. Reverted to upstream's total timeout. The idle timeout was for Snowflake compatibility which has been removed.

3. **`tor-dirmgr/src/bootstrap.rs`** — Streaming downloads with `#[cfg(test)]`/`#[cfg(not(test))]` split giving zero test coverage for production path. Reverted to upstream's batch approach. See `potential-improvements.md` for future work.

4. **`tor-dirmgr/src/docid.rs`** — Dead `MICRODESC_N` constant (always equal to `N`). Reverted to upstream.

5. **`tor-hsclient/src/pow/v1.rs`, `tor-hsservice/src/timeout_track.rs`, `tor-hsservice/src/time_store.rs`** — Missed `std::time::Instant` → `tor_time::Instant` migrations. Fixed.

6. **`tor-dirmgr/src/storage/custom.rs`** — Dead `str_to_flavor()` function with `#[allow(dead_code)]`. Removed (zero callers anywhere).

7. **`tor-chanmgr/src/factory.rs`** — Unused `BootstrapReporter` methods (`record_attempt`, etc.) added for custom ChannelFactory impls that don't exist. Reverted.

8. **`tor-chanmgr/src/lib.rs`** — Unnecessary `#[allow(unused)]` → `#[cfg_attr(...)]` attr change. Reverted.

9. **`tor-dirclient/src/request.rs`** — WASM-specific `max_response_len` override (16KB vs 8KB). Reverted (belongs in WASM branch if needed).

10. **`tor-dircommon/src/retry.rs`** — WASM-specific parallelism reduction. Reverted (belongs in WASM branch).

11. **`tor-config-path/src/addr.rs`** — Trivial clippy `return` removal. Reverted.

12. **`tor-cert/tests/invalid_certs.rs`** — Cosmetic commented-out import edit. Reverted.

13. Multiple files identical to upstream after wasm-basic-compat migration artifacts were identified and removed from the analysis (hashx/bench, tor-netdir/testnet+testprovider, tor-proto/maybenot+rtt+initiator+responder, tor-rtmock/tests, retry-error, tor-circmgr/preemptive, tor-dirserver/operation, tor-hsservice/rend_handshake).
