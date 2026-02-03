# SystemTime Cross-Platform Migration TODO

## Summary

The codebase has been migrated to use `tor_rtcompat::SystemTime` (which re-exports `web_time::SystemTime` on WASM and `std::time::SystemTime` on native) for cross-platform compatibility.

## Current State

**WASM build compiles and runs.** The client successfully:
- Connects via WebSocket to Snowflake
- Establishes TLS through the bridge
- Completes Tor channel handshake
- Attempts directory bootstrap (fails with "Partial response" - separate issue)

## Remaining Cleanup

### Conversions to `std::time::SystemTime`

Some external crates require `std::time::SystemTime`, necessitating conversions at boundaries:

1. **`httpdate` crate** - `httpdate::fmt_http_date()` requires `std::time::SystemTime`
   - Location: `crates/tor-dirclient/src/request.rs`
   - Uses `to_std_systemtime()` helper
   - Required for HTTP `If-Modified-Since` headers

2. **`humantime` crate** - `humantime::format_rfc3339_seconds()` requires `std::time::SystemTime`
   - Location: `crates/tor-dirmgr/src/bridgedesc.rs`
   - Uses `to_std_systemtime()` helper
   - Could be replaced with `time` crate's RFC3339 formatter (like in `crates/arti-client/src/protostatus.rs`)

### Pattern for Cross-Platform Time Formatting

Instead of using `humantime` with conversion, prefer:

```rust
fn format_rfc3339(t: SystemTime) -> String {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(std::time::Duration::ZERO)
        .as_secs();
    OffsetDateTime::from_unix_timestamp(secs as i64)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
        .format(&Rfc3339)
        .unwrap_or_else(|_| "<time format error>".into())
}
```

### Pattern for `OffsetDateTime` Conversion

When interfacing with the `time` crate's `OffsetDateTime`:

```rust
// SystemTime -> OffsetDateTime
fn systemtime_to_odt(t: SystemTime) -> OffsetDateTime {
    let duration = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);
    OffsetDateTime::from_unix_timestamp(duration.as_secs() as i64)
        .unwrap_or(OffsetDateTime::UNIX_EPOCH)
}

// OffsetDateTime -> SystemTime
fn odt_to_systemtime(odt: OffsetDateTime) -> SystemTime {
    let secs = odt.unix_timestamp();
    let nanos = odt.nanosecond();
    let duration = std::time::Duration::new(secs as u64, nanos);
    SystemTime::UNIX_EPOCH + duration
}
```

## Files With Helper Functions

- `crates/tor-dirclient/src/request.rs` - `to_std_systemtime()`
- `crates/tor-dirmgr/src/bridgedesc.rs` - `to_std_systemtime()`
- `crates/tor-dirmgr/src/state.rs` - `systemtime_to_odt()`
- `crates/tor-dirmgr/src/storage/inmemory.rs` - `time_duration_to_std()`
- `crates/arti-client/src/protostatus.rs` - `format_rfc3339()`

## Separate Issue: Directory Bootstrap

The "Partial response" error during directory download is unrelated to SystemTime migration. The transport layer works correctly.