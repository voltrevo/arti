#![no_main]
use libfuzzer_sys::fuzz_target;
use tor_netdoc::doc::microdesc::{Microdesc, MicrodescReader};
use tor_netdoc::AllowAnnotations;
use tor_netdoc::parse2::{parse_netdoc_multiple, ParseInput};

fuzz_target!(|data: (bool, &str)| {
    let allow = if data.0 {
        AllowAnnotations::AnnotationsAllowed
    } else {
        AllowAnnotations::AnnotationsNotAllowed
    };

    if let Ok(md) = MicrodescReader::new(data.1, &allow) {
        for _ in md {}
    }

    let input = ParseInput::new(data.1, "<none>");
    let _ignore = parse_netdoc_multiple::<Microdesc>(&input);
});
