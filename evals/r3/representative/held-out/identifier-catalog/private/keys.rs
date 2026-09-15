pub fn normalize(label: &str) -> Option<String> {
    if label.is_empty() || label.len() > 128 {
        return None;
    }
    let mapped: Vec<char> = label
        .chars()
        .map(|c| match c {
            'Ａ'..='Ｚ' => char::from_u32(c as u32 - 'Ａ' as u32 + 'a' as u32).unwrap(),
            'ａ'..='ｚ' => char::from_u32(c as u32 - 'ａ' as u32 + 'a' as u32).unwrap(),
            'A'..='Z' => c.to_ascii_lowercase(),
            'É' => 'é',
            other => other,
        })
        .collect();
    let mut chars = mapped.into_iter().peekable();
    let mut result = String::new();
    let mut length = 0;
    while let Some(mut c) = chars.next() {
        if c == 'e' && chars.peek() == Some(&'\u{0301}') {
            chars.next();
            c = 'é';
        }
        if !(c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-' | 'é')) {
            return None;
        }
        result.push(c);
        length += 1;
        if length > 32 {
            return None;
        }
    }
    (!result.is_empty()).then_some(result)
}
