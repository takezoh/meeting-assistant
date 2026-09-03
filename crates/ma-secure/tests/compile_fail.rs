//! Compile-fail witnesses: a raw secret cannot be displayed or serialized, and meeting content
//! cannot be attached to a log field. If any of these ever compiles, the property regressed.

#[test]
fn secret_cannot_be_displayed() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/secret_display.rs");
    t.compile_fail("tests/ui/secret_serialize.rs");
}

#[test]
fn content_type_cannot_be_logged() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/content_log_field.rs");
    t.compile_fail("tests/ui/content_display.rs");
}
