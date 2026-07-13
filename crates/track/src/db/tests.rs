use super::*;

#[test]
fn migrations_apply_on_fresh_db() {
    let c = OpenOptions::new()
        .extended_result_codes()
        .read_write()
        .create()
        .no_mutex()
        .open_in_memory()
        .unwrap();

    do_migrations(&c).expect("migrations should apply");
}
