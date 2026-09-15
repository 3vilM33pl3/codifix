pub fn apply(value: i64, numerator: i64, denominator: i64) -> Option<i64> {
    Some(((value as f64) * numerator as f64 / denominator as f64).round() as i64)
}
