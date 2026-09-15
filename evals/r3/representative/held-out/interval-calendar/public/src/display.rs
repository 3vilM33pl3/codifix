pub fn spans(label: &str, intervals: &[crate::interval::Interval]) -> String {
    format!(
        "{label}:{}",
        intervals
            .iter()
            .map(|interval| format!("{}:{}", interval.start, interval.end))
            .collect::<Vec<_>>()
            .join(",")
    )
}
