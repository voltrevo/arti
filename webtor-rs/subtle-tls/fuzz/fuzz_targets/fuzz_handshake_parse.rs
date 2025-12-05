#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct HandshakeMessage {
    msg_type: u8,
    payload: Vec<u8>,
}

fuzz_target!(|msg: HandshakeMessage| {
    // Build a handshake message with proper header
    let len = msg.payload.len();
    if len > 0xFFFFFF {
        return; // Skip if payload is too large for 24-bit length
    }
    
    let mut data = Vec::with_capacity(4 + len);
    data.push(msg.msg_type);
    data.push((len >> 16) as u8);
    data.push((len >> 8) as u8);
    data.push(len as u8);
    data.extend(&msg.payload);
    
    // Parse the handshake header
    let _ = subtle_tls::handshake::parse_handshake_header(&data);
    
    // Try parsing as different message types based on msg_type
    match msg.msg_type {
        2 => {
            // ServerHello - would need HandshakeState
        }
        11 => {
            // Certificate
            let _ = subtle_tls::handshake::parse_certificate(&msg.payload);
        }
        15 => {
            // CertificateVerify
            let _ = subtle_tls::handshake::parse_certificate_verify(&msg.payload);
        }
        20 => {
            // Finished
            let _ = subtle_tls::handshake::parse_finished(&msg.payload);
        }
        _ => {}
    }
});
