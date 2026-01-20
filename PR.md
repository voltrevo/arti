# Add WASM Support to Arti

wasm does not support:
- `std::time::Instant`
- `coarsetime::Instant`

This PR fixes that by:
1. `tor_rtcompat` now exports `Instant` (from `web_time::Instant`) which works on both native and wasm (`web_time::Instant` is `std::time::Instant` on non-wasm)
2. `tor_rtcompat::CoarseInstant` now has a wasm version
3. All crates use `tor_rtcompat::Instant` or `tor_rtcompat::CoarseInstant` instead of direct `std::time::Instant` or `coarsetime::Instant`

(`web_time::Instant` is just `std::time::Instant` on non-wasm platforms.)

Additionally:
- added wasm impl of `AtomicOptTimestamp`
- `is_not_a_directory` now returns `false` on wasm (and other non-unix/non-windows platforms)
- `socket2` dependency moved to non-wasm targets only (wasm doesn't support TCP listening)
- `tcp_listen` is omitted in wasm builds (wasm doesn't support TCP listening)

These changes have been tested in wasm in the webtor-rs project.
