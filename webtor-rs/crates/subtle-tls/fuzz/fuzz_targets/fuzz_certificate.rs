#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz TLS 1.3 certificate parsing
    let _ = subtle_tls::handshake::parse_certificate(data);
    
    // Fuzz TLS 1.2 certificate parsing
    #[cfg(feature = "tls12")]
    let _ = subtle_tls::handshake_1_2::parse_certificate(data);
});
