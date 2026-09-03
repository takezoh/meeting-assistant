//! The shared adapter conformance suite, run against this adapter's own table.

use ma_signal::adapter::{conformance_violations, MeetingAdapter};

#[test]
fn conformance() {
    let adapter = ma_adapter_slack::adapter();
    let violations = conformance_violations(&adapter);
    assert!(violations.is_empty(), "{violations:#?}");
    assert_eq!(adapter.id(), adapter.spec().id);
}
