#![no_main]
use libfuzzer_sys::fuzz_target;
use tor_netdoc::doc::authcert::{AuthCert, AuthCertUnverified};
use tor_netdoc::parse2::{parse_netdoc_multiple, ParseInput};

fuzz_target!(|data: &str| {
    if let Ok(certs) = AuthCert::parse_multiple(data) {
        for _ in certs {}
    }

    let input = ParseInput::new(data, "<none>");
    let _ = parse_netdoc_multiple::<AuthCertUnverified>(&input);
});
