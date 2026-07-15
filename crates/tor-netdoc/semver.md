ADDED: Derive `PartialEq` and `Eq` on `RouterDescSignatures`
ADDED: Derive `PartialEq` and `Eq` on `RouterDesc`
ADDED: Derive `PartialEq` and `Eq` on (embedded) certificate types
BREAKING: `Ed25519NtorCrossCert::new_signed()` now accepts an `&ExpandedKeypair`
ADDED: Various `parse2` and `encode` traits for `Intern<T>` wrapped types
BREAKING: `SoftwareVersion::Other` now stores a `String`
BREAKING: `Microdesc::ipv4_policy` now stored in `Intern`
BREAKING: `Microdesc::ipv6_policy` now stored in `Intern`
BREAKING: `RouterDesc::ipv6_policy` now stored in `Intern`
BREAKING: `PortPolicy::intern()` now returns an `Intern`
BREAKING: `Microdesc::family` now stored in `Intern`
BREAKING: `RouterDesc::family` now stored in `Intern`
BREAKING: `RelayFamily::intern()` now returning an `Intern`
BREAKING: Ed25519 certificate verify methods now wrap around `TimerangeBound`
ADDED: `NetdocParseableUnverified` for `RouterDesc`
ADDED: `NetdocEncodableFields` for `AddrPolicy`
ADDED: `encode::ItemArgument` for `SpFingerprint`
ADDED: `ItemValueEncodable` for `ExtraInfoDigests`
ADDED: `ItemValueEncodable` for `RouterDescIntroItem`
ADDED: `ItemValueEncodable` for `OverloadGeneral`
ADDED: `ItemValueEncodable` for `RelayPlatform`
BREAKING: Replaced `ItemArgumentParseable` with `ItemValueParseable` for `RelayPlatform`
ADDED: `NtorOnionKeyCrossCert` type
ADDED: `RouterDesc::ntor_onion_key_crosscert`
ADDED: `HiddenServiceDirToken`
ADDED: `RouterDesc::hidden_service_dir`
ADDED: `CachesExtraInfoToken`
ADDED: `TunnelledDirServerToken`
BREAKING: `RouterDesc::caches_extra_info` stored as `CachesExtraInfoToken`
BREAKING: `RouterDesc::tunnelled_dir_server` stored as `TunnelledDirServerToken`
ADDED: `Ed25519NtorCrossCert` type
ADDED: `ItemPresent` type
ADDED: `RouterDesc::extra_info_digest` field
ADDED: `RouterDesc::contact` field
ADDED: `RouterDesc::overload_general` field
ADDED: `RouterDesc::hibernating` field
ADDED: `Display` for `RelayPlatform`
BREAKING: `RelayPlatform::Tor` now stores the platform as `Option<String>`
BREAKING: `NetdocUnverified` trait is now `NetdocParseableUnverified`
ADDED: `ParseInput` improved: accessors, `retain_unknown_values`, `Clone`, `Debug`
ADDED: `RouterStatus` and `RouterStatusIntroItem` implement netdoc encoding traits
ADDED: `RouterStatus` and `netstatus::Signature` implement `EncodeOrd`
ADDED: `F64Finite` type
ADDED: `doc::netstatus::{plain, md, vote}::NetworkStatus`
BREAKING: `AuthCertUnverified::verify` doesn't take times; instead returns `TimerangeBound`
DEPRECATED: `parse2::check_validity_time` and `check_validity_time_tolerance`
ADDED: `impl From<std::convert::Infallible> for Error`
ADDED: `RouterStatus` fields `r.dir_port`, `p`, `id`, `stats`
ADDED: `plain::NetworkStatus` and `md::NetworkStatus` implement `NetdocEncodable`
ADDED: `plain::NetworkStatus` and `md::NetworkStatus` have `verify` methods
ADDED: `EmbeddedCert` implements `NetdocEncodable` and `NetdocParseable`
ADDED: `Microdesc`, `RelayFamily`, `RelayFamilyIds` are encodable
ADDED: `MicrodescConstructor`
ADDED: `NetdocEncodable` derive, `#[deftly(netdoc(default(skip)))]` option
ADDED: `MicrodescIntroItem`
BREAKING: `Microdesc` split into `Microdesc` and `MicrodescAndHash`
