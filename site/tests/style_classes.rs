use sockel_build::Usage;

include!("../style.rs");

#[test]
fn every_styled_class_is_written() {
    Usage::new()
        .source(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .source(concat!(env!("CARGO_MANIFEST_DIR"), "/manual.rs"))
        .assert_clean(&style().style(env!("SOCKEL_STYLE")));
}
