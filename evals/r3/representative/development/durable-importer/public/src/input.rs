#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Delivery {
    pub seq: u64,
    pub id: String,
    pub delta: i64,
}

pub fn parse(parts: &[&str]) -> Option<(Delivery, String)> {
    if parts.len() != 5 || parts[0] != "apply" {
        return None;
    }
    let seq = parts[1].parse().ok()?;
    if seq == 0
        || parts[2].is_empty()
        || !parts[2].bytes().all(|b| b.is_ascii_alphanumeric())
        || !["none", "before", "after"].contains(&parts[4])
    {
        return None;
    }
    Some((
        Delivery {
            seq,
            id: parts[2].into(),
            delta: parts[3].parse().ok()?,
        },
        parts[4].into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_delivery_does_not_become_an_operation() {
        assert!(parse(&["apply", "0", "a", "1", "none"]).is_none());
        assert!(parse(&["apply", "1", "a", "oops", "none"]).is_none());
        assert!(parse(&["apply", "1", "a", "1", "unknown"]).is_none());
    }
}
