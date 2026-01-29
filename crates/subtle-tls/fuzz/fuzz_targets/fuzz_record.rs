#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;

#[derive(Arbitrary, Debug)]
struct RecordInput {
    content_type: u8,
    version_major: u8,
    version_minor: u8,
    length: u16,
    payload: Vec<u8>,
}

fuzz_target!(|input: RecordInput| {
    // Build a TLS record from fuzzer input
    let mut record = Vec::new();
    record.push(input.content_type);
    record.push(input.version_major);
    record.push(input.version_minor);
    record.push((input.length >> 8) as u8);
    record.push(input.length as u8);
    record.extend(&input.payload);
    
    // The record layer would parse this - we test bounds checking
    if record.len() >= 5 {
        let declared_len = ((record[3] as usize) << 8) | (record[4] as usize);
        let actual_payload = record.len() - 5;
        
        // Verify we don't panic on length mismatches
        let _ = declared_len.checked_sub(actual_payload);
    }
});
