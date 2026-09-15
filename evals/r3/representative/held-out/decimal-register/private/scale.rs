pub fn apply(value: i64, numerator: i64, denominator: i64) -> Option<i64> {
    if !(-1_000_000..=1_000_000).contains(&numerator) || !(1..=1_000_000).contains(&denominator) {
        return None;
    }
    let product = i128::from(value) * i128::from(numerator);
    let divisor = i128::from(denominator);
    let mut rounded = product / divisor;
    if (product % divisor).abs() * 2 >= divisor {
        rounded += if product < 0 { -1 } else { 1 };
    }
    i64::try_from(rounded).ok()
}
