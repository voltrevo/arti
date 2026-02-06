//! Cross-platform serde helpers for SystemTime.
//!
//! This module provides human-readable serialization for `SystemTime` that works
//! on both native and WASM platforms.
//!
//! On native platforms, it uses `humantime_serde` for human-readable format.
//! On WASM, it uses a duration-since-epoch format that's compatible with
//! `web_time::SystemTime`.

use tor_time::SystemTime;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::time::Duration;

/// Serialize a `SystemTime` in human-readable format.
///
/// Use with `#[serde(with = "tor_rtcompat::serde_time")]`
pub fn serialize<S>(time: &SystemTime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        // On native, use humantime_serde for human-readable format
        humantime_serde::serialize(time, serializer)
    }

    #[cfg(target_arch = "wasm32")]
    {
        // On WASM, serialize as duration since epoch (as string for human readability)
        let duration = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        let secs = duration.as_secs();
        let nanos = duration.subsec_nanos();
        let s = if nanos == 0 {
            format!("{}s", secs)
        } else {
            format!("{}.{}s", secs, nanos)
        };
        s.serialize(serializer)
    }
}

/// Deserialize a `SystemTime` from human-readable format.
///
/// Use with `#[serde(with = "tor_rtcompat::serde_time")]`
pub fn deserialize<'de, D>(deserializer: D) -> Result<SystemTime, D::Error>
where
    D: Deserializer<'de>,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        // On native, use humantime_serde for human-readable format
        humantime_serde::deserialize(deserializer)
    }

    #[cfg(target_arch = "wasm32")]
    {
        // On WASM, deserialize as duration string or seconds
        use serde::de::Error;

        let s = String::deserialize(deserializer)?;

        // Try to parse as humantime format first (e.g., "2023-07-05T11:25:56Z")
        // Then fall back to simple duration format (e.g., "12345s" or "12345.678s")
        if let Ok(std_time) = humantime::parse_rfc3339(&s) {
            // humantime returns std::time::SystemTime, so use std UNIX_EPOCH
            let duration = std_time
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or(Duration::ZERO);
            return Ok(SystemTime::UNIX_EPOCH + duration);
        }

        // Try simple seconds format
        let s = s.trim_end_matches('s');
        if let Some((secs_str, nanos_str)) = s.split_once('.') {
            let secs: u64 = secs_str.parse().map_err(D::Error::custom)?;
            let nanos: u32 = nanos_str.parse().map_err(D::Error::custom)?;
            Ok(SystemTime::UNIX_EPOCH + Duration::new(secs, nanos))
        } else {
            let secs: u64 = s.parse().map_err(D::Error::custom)?;
            Ok(SystemTime::UNIX_EPOCH + Duration::from_secs(secs))
        }
    }
}

/// Module for serializing `Option<SystemTime>`.
///
/// Use with `#[serde(with = "tor_rtcompat::serde_time::option")]`
pub mod option {
    use super::*;

    /// Serialize an `Option<SystemTime>` in human-readable format.
    pub fn serialize<S>(time: &Option<SystemTime>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match time {
            Some(t) => serializer.serialize_some(&SerializeWrapper(t)),
            None => serializer.serialize_none(),
        }
    }

    /// Deserialize an `Option<SystemTime>` from human-readable format.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<SystemTime>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<DeserializeWrapper>::deserialize(deserializer).map(|opt| opt.map(|w| w.0))
    }

    /// Wrapper for serialization
    struct SerializeWrapper<'a>(&'a SystemTime);

    impl Serialize for SerializeWrapper<'_> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            super::serialize(self.0, serializer)
        }
    }

    /// Wrapper for deserialization
    struct DeserializeWrapper(SystemTime);

    impl<'de> Deserialize<'de> for DeserializeWrapper {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            super::deserialize(deserializer).map(DeserializeWrapper)
        }
    }
}