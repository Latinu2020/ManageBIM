#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = bimrust::e57::parse_las(data, &|_, _| {});
});
