pub fn disclose(
    record: &crate::record::Record,
    _destination: &str,
) -> Result<String, &'static str> {
    let _ = &record.kind;
    Ok(record
        .fields
        .iter()
        .filter(|(key, _)| key.as_str() != "auth.token")
        .map(|(key, value)| format!("{key}={}", crate::record::encode(value)))
        .collect::<Vec<_>>()
        .join(";"))
}
