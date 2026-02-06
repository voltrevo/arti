# Improve Bootstrap Progress Info Logs

## Summary
Add info-level logging that shows bootstrap progress with percentages and phase descriptions. Currently, progress updates flow through `update_progress()` but are never logged - only broadcast to event subscribers.

## Current State
- [lib.rs:850-856](crates/tor-dirmgr/src/lib.rs#L850-L856) - `update_progress()` broadcasts progress but doesn't log
- [bootstrap.rs:636](crates/tor-dirmgr/src/bootstrap.rs#L636) - Logs attempt number + `state.describe()` (not the same as DirProgress)
- [event.rs:442-493](crates/tor-dirmgr/src/event.rs#L442-L493) - `DirProgress` has excellent Display impl already:
  - `"fetching a consensus"`
  - `"fetching authority certificates (3/5)"`
  - `"fetching microdescriptors (30/40)"`
  - `"usable, fresh until ..., and valid until ..."`

## Implementation

### 1. Add progress logging in `update_progress()` (lib.rs:850-856)

Add logging for:
- Phase transitions (NoConsensus -> FetchingCerts -> Validated -> usable)
- **Periodic updates during microdesc download**: every 10% OR every 5 seconds (whichever comes first)

Add fields to DirMgr to track logging state:
```rust
// In DirMgr struct
last_logged_pct: AtomicU8,
last_logged_time: Mutex<Instant>,
```

Then modify `update_progress()`:
```rust
fn update_progress(&self, attempt_id: AttemptId, progress: DirProgress) {
    let mut sender = self.send_status.lock().expect("poisoned lock");
    let mut status = sender.borrow_mut();

    status.update_progress(attempt_id, progress.clone());

    // Calculate current percentage
    let frac = status.current().map(|s| s.frac()).unwrap_or(0.0);
    let pct = (frac * 100.0).round() as u8;

    // Log if:
    // 1. We crossed a 10% threshold, OR
    // 2. 5 seconds have passed since last log (during microdesc phase)
    let last_pct = self.last_logged_pct.load(Ordering::Relaxed);
    let crossed_threshold = pct / 10 > last_pct / 10;

    let time_elapsed = {
        let last_time = self.last_logged_time.lock().expect("poisoned");
        last_time.elapsed() >= Duration::from_secs(5)
    };

    let in_microdesc_phase = matches!(progress, DirProgress::Validated { usable: false, .. });

    if crossed_threshold || (in_microdesc_phase && time_elapsed) {
        self.last_logged_pct.store(pct, Ordering::Relaxed);
        *self.last_logged_time.lock().expect("poisoned") = Instant::now();
        info!("Bootstrap {}%: {}", pct, progress);
    }
}
```

This ensures users see progress every 5 seconds during microdesc download even if percentage changes slowly.

### 2. Demote redundant log in bootstrap.rs:636

Change from info to debug since `update_progress` now handles progress logging:
```rust
// Before: info!(attempt=%attempt_id, "{}: {}", attempt + 1, state.describe());
debug!(attempt=%attempt_id, "Download attempt {}: {}", attempt + 1, state.describe());
```

## Expected Output

```
INFO tor_dirmgr: Bootstrap 0%: fetching a consensus
INFO tor_dirmgr: Bootstrap 30%: fetching authority certificates (3/5)
INFO tor_dirmgr: Bootstrap 40%: fetching microdescriptors (125/2500)
INFO tor_dirmgr: Bootstrap 50%: fetching microdescriptors (375/2500)
INFO tor_dirmgr: Bootstrap 60%: fetching microdescriptors (625/2500)
INFO tor_dirmgr: Bootstrap 70%: fetching microdescriptors (875/2500)
INFO tor_dirmgr: Bootstrap 80%: fetching microdescriptors (1125/2500)
INFO tor_dirmgr: Bootstrap 90%: fetching microdescriptors (1375/2500)
INFO tor_dirmgr: Bootstrap 100%: usable, fresh until 2024-01-15 12:00:00 UTC, and valid until 2024-01-15 15:00:00 UTC
INFO tor_dirmgr: We have enough information to build circuits.
```

During microdesc download (the longest phase, 35-100%), you'll see progress every ~10% OR every 5 seconds, whichever comes first.

## Files to Modify
- [crates/tor-dirmgr/src/lib.rs](crates/tor-dirmgr/src/lib.rs) - Add `last_logged_pct` and `last_logged_time` fields to DirMgr, add logging in `update_progress()`
- [crates/tor-dirmgr/src/bootstrap.rs](crates/tor-dirmgr/src/bootstrap.rs) - Demote line 636 from info to debug

## Verification
1. Run `examples/tor-fetch-with-storage.js` and observe info logs show progress with percentages
2. Run with `RUST_LOG=debug` to verify the demoted log still appears at debug level
