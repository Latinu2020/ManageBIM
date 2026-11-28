#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Must never panic or abort, no matter what the header claims.
    let _ = bimrust::hdri::parse(data, "fuzz.hdr");
});
