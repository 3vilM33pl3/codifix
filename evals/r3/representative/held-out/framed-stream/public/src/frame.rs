pub const MAX_PAYLOAD: usize = 8;

pub fn first(bytes: &[u8]) -> Result<Option<(Vec<u8>, usize)>, &'static str> {
    if bytes.len() < 2 {
        return Ok(None);
    }
    let length = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    if length > MAX_PAYLOAD {
        return Err("length");
    }
    if bytes.len() < length + 2 {
        return Ok(None);
    }
    let payload = &bytes[2..length + 2];
    std::str::from_utf8(payload).map_err(|_| "utf8")?;
    Ok(Some((payload.to_vec(), length + 2)))
}
