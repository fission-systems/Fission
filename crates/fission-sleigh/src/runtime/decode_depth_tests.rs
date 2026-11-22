//! Constructor nesting is bounded by a parse depth, not by the input.
//!
//! A SLEIGH table that refers to itself -- x86's `instruction` table takes a
//! prefix byte and descends into `instruction` again -- nests one constructor
//! per matching byte. Without a depth bound the decoder recursed once per byte
//! and a long run of prefixes overflowed the stack (found by the
//! `fuzz_sleigh_decode` target).

use super::*;

/// Decode on a thread with a fixed stack so the old unbounded recursion fails
/// the same way on every machine instead of depending on the test harness's
/// thread size.
fn decode_on_bounded_stack(language: &'static str, bytes: Vec<u8>) -> Result<(Vec<PcodeOp>, u64)> {
    std::thread::Builder::new()
        .stack_size(8 << 20)
        .spawn(move || {
            let frontend = RuntimeSleighFrontend::new_for_language(language).unwrap();
            frontend.decode_and_lift_with_len(&bytes, 0x1000)
        })
        .unwrap()
        .join()
        .unwrap()
}

#[test]
fn a_long_prefix_run_is_a_decode_error_not_a_stack_overflow() {
    // 0x65 is the GS segment override; every one of them matches the prefix
    // constructor that re-enters the `instruction` table.
    let result = decode_on_bounded_stack("x86-64", vec![0x65; 50_000]);
    let err = result.expect_err("50,000 prefixes are not one instruction");
    assert!(
        format!("{err:#}").contains("maximum parse depth"),
        "the error should name the depth bound, got: {err:#}"
    );
}

#[test]
fn the_longest_legal_x86_instruction_still_decodes() {
    // `data16` x6, `cs`, then `nopw 0x0(%rax,%rax,1)`: the 15-byte NOP that
    // assemblers emit as padding. Fifteen bytes is x86's architectural limit,
    // so the depth bound must leave room for it.
    let bytes = vec![
        0x66, 0x66, 0x66, 0x66, 0x66, 0x66, 0x2e, 0x0f, 0x1f, 0x84, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let (_, len) = decode_on_bounded_stack("x86-64", bytes).expect("15-byte nop decodes");
    assert_eq!(len, 15);
}

#[test]
fn fourteen_prefixes_are_one_level_each_and_still_decode() {
    // The most prefixes a 15-byte instruction can carry, on the same
    // self-referencing constructor the overflow went through.
    let mut bytes = vec![0x65; 14];
    bytes.push(0x90);
    let (_, len) = decode_on_bounded_stack("x86-64", bytes).expect("14 prefixes + nop decode");
    assert_eq!(len, 15);
}
