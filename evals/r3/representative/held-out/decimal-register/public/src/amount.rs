pub fn parse(text: &str) -> Option<i64> {
    let value: f64 = text.parse().ok()?;
    value.is_finite().then_some((value * 1000.0).round() as i64)
}

pub fn display(value: i64) -> String {
    let wide = i128::from(value);
    let sign = if wide < 0 { "-" } else { "" };
    let magnitude = wide.abs();
    format!("{sign}{}.{:03}", magnitude / 1000, magnitude % 1000)
}
