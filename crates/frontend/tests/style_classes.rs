//! Every class track's Sass defines is written by its markup
//! (`--nocapture` prints the counts).

use sockel_build::Usage;

include!("../style.rs");

#[test]
fn every_styled_class_is_written() {
    Usage::new()
        .source(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        // The help pages name icons at runtime (`icon:` links).
        .dynamic(["track-icon-*"])
        .assert_clean(&style().style(env!("SOCKEL_STYLE")));
}
