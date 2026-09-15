pub fn parse(text: &str) -> Option<i64> {
    if text.is_empty() || text.len() > 40 || !text.is_ascii() {
        return None;
    }
    let (negative, digits) = match text.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, text),
    };
    let (whole, fraction) = match digits.split_once('.') {
        Some((whole, fraction)) if !fraction.is_empty() && fraction.len() <= 3 => (whole, fraction),
        Some(_) => return None,
        None => (digits, ""),
    };
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let integer: i128 = whole.parse().ok()?;
    let fractional: i128 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().ok()?
    };
    let magnitude = integer
        .checked_mul(1000)?
        .checked_add(fractional * 10_i128.pow(3 - fraction.len() as u32))?;
    i64::try_from(if negative {
        magnitude.checked_neg()?
    } else {
        magnitude
    })
    .ok()
}

pub fn display(value: i64) -> String {
    let wide = i128::from(value);
    let sign = if wide < 0 { "-" } else { "" };
    let magnitude = wide.abs();
    format!("{sign}{}.{:03}", magnitude / 1000, magnitude % 1000)
}
