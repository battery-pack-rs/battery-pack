#![no_main]
use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use {{ crate_name }}::add;

#[derive(Debug, Arbitrary)]
struct FuzzInput {
    left: u64,
    right: u64,
}

fuzz_target!(|input: FuzzInput| {
    // TODO: Replace with your crate's API.
    let _ = add(input.left, input.right);
});
