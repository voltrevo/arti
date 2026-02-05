# tor-time

Cross-platform time types and utilities for Tor.

## Overview

This crate provides time types that work on both native platforms and WASM:

- `SystemTime`, `Instant`, `Duration`, `UNIX_EPOCH` - via `web_time`
- `CoarseInstant`, `CoarseDuration`, `CoarseTimeProvider` - cheap monotonic time
- Utility functions for time formatting and conversion

## Architecture

`tor-time` sits at the bottom of the dependency chain, allowing all crates to use consistent time types:

```
tor-time (lowest level)
    ↓
retry-error
    ↓
tor-error
    ↓
tor-rtcompat (re-exports tor-time)
    ↓
all other crates
```

## Usage

Most code should use `tor_rtcompat` which re-exports everything:

```rust
use tor_rtcompat::{SystemTime, Instant, Duration, UNIX_EPOCH};
use tor_rtcompat::{format_rfc3339, fmt_http_date, time_duration_to_std};

// Format times for display
let now = SystemTime::now();
println!("{}", format_rfc3339(now));  // "2024-01-15T10:30:00Z"
println!("{}", fmt_http_date(now));   // "Mon, 15 Jan 2024 10:30:00 GMT"

// Convert time::Duration (from config parsing) to std::time::Duration
let config_duration: time::Duration = time::Duration::hours(1);
let std_duration = time_duration_to_std(config_duration);
```

## Platform Differences

| Type | Native | WASM |
|------|--------|------|
| `SystemTime` | `std::time::SystemTime` | `web_time::SystemTime` (JS Date) |
| `Instant` | `std::time::Instant` | `web_time::Instant` (performance.now) |
| `Duration` | `std::time::Duration` | `std::time::Duration` (same) |
| `CoarseInstant` | `coarsetime::Instant` | `web_time::Instant` |

## License

MIT OR Apache-2.0
