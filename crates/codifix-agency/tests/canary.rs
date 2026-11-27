use codifix_agency::improvement::*;
use uuid::Uuid;
#[test]
fn canary_fraction_is_bounded_and_stable_for_a_task() {
    let candidate = Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").expect("candidate");
    // Fixed protocol vector: UUIDv5(candidate, task bytes) bucket is 33790.
    let vector_task = Uuid::from_u128(3);
    assert!(!canary_selected(candidate, vector_task, 33_790).expect("boundary excluded"));
    assert!(canary_selected(candidate, vector_task, 33_791).expect("boundary included"));
    let task = Uuid::from_u128(1);
    let selected = canary_selected(candidate, task, 100_000).expect("canary");
    for _ in 0..10 {
        assert_eq!(
            canary_selected(candidate, task, 100_000).expect("stable canary"),
            selected
        );
    }
    assert!(!canary_selected(candidate, task, 0).expect("zero fraction"));
    assert!(canary_selected(candidate, task, 100_001).is_err());
    assert!(canary_selected(Uuid::nil(), task, 1000).is_err());
    assert_eq!(
        serde_json::to_value(ImprovementKind::Procedure).expect("kind"),
        "procedure"
    );
}
