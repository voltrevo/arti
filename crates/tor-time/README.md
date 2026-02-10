# tor-time

Cross-platform time types and utilities for Tor.

## Overview

This crate provides time types that work on both native platforms and WASM:

- `SystemTime`, `Instant`, `Duration`, `UNIX_EPOCH` - via `web_time`
- `CoarseInstant`, `CoarseDuration`, `CoarseTimeProvider` - cheap monotonic time
- Utility functions for time formatting and conversion

## Platform Differences

| Type | Native | WASM |
|------|--------|------|
| `SystemTime` | `std::time::SystemTime` | `web_time::SystemTime` (JS Date) |
| `Instant` | `std::time::Instant` | `web_time::Instant` (performance.now) |
| `Duration` | `std::time::Duration` | `std::time::Duration` (same) |
| `CoarseInstant` | `coarsetime::Instant` | `web_time::Instant` |

## License

MIT OR Apache-2.0
