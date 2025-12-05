#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Fuzz TLS 1.3 handshake header parsing
    let _ = subtle_tls::handshake::parse_handshake_header(data);
    
    // Fuzz TLS 1.3 Finished message parsing  
    let _ = subtle_tls::handshake::parse_finished(data);
    
    // Fuzz TLS 1.3 CertificateVerify parsing
    let _ = subtle_tls::handshake::parse_certificate_verify(data);
});
