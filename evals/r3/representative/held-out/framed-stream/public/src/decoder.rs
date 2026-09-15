#[derive(Default)]
pub struct Decoder {
    delivered: usize,
}

impl Decoder {
    pub fn feed(&mut self, bytes: &[u8]) -> String {
        match crate::frame::first(bytes) {
            Ok(Some((payload, _))) => {
                self.delivered += 1;
                format!("frames:{}", crate::hex::encode(&payload))
            }
            Ok(None) => "frames:".into(),
            Err(reason) => format!("error:{reason}"),
        }
    }

    pub fn end(&mut self) -> String {
        "closed".into()
    }

    pub fn status(&self) -> String {
        format!("pending:0 delivered:{} state:open", self.delivered)
    }
}
