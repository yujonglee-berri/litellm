#[test]
fn interaction_delivery_is_closed_to_the_framework_markers() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/invalid_delivery.rs");
}
