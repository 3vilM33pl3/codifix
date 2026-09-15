use crate::{command::Command, store::Store};
#[derive(Clone)]
struct Stream {
    limit: usize,
    encoded: usize,
    pending: Option<u8>,
    staged: Vec<u8>,
}
#[derive(Default)]
pub struct Decoder {
    store: Store,
    stream: Option<Stream>,
    failed: bool,
}
impl Decoder {
    fn fail(&mut self, error: &str) -> String {
        self.stream = None;
        self.failed = true;
        error.into()
    }
    pub fn run(&mut self, command: Command) -> String {
        match command {
            Command::Begin(limit) => {
                self.stream = Some(Stream {
                    limit,
                    encoded: 0,
                    pending: None,
                    staged: Vec::new(),
                });
                self.failed = false;
                "ready".into()
            }
            Command::Show => self.store.show(),
            Command::Status => {
                if self.failed {
                    "failed".into()
                } else {
                    self.stream
                        .as_ref()
                        .map(|s| {
                            format!(
                                "active in:{} out:{} pending:{}",
                                s.encoded,
                                s.staged.len(),
                                if s.pending.is_some() { "yes" } else { "no" }
                            )
                        })
                        .unwrap_or_else(|| "idle".into())
                }
            }
            Command::Feed(bytes) => {
                let Some(mut stream) = self.stream.clone() else {
                    return "inactive".into();
                };
                if stream.encoded + bytes.len() > 32 {
                    return self.fail("limit");
                }
                stream.encoded += bytes.len();
                for byte in bytes {
                    if let Some(count) = stream.pending.take() {
                        let count = usize::from(count);
                        if stream.staged.len() + count > stream.limit {
                            return self.fail("limit");
                        }
                        stream.staged.extend(std::iter::repeat_n(byte, count));
                    } else {
                        if byte == 0 {
                            return self.fail("malformed");
                        }
                        stream.pending = Some(byte);
                    }
                }
                self.stream = Some(stream);
                "queued".into()
            }
            Command::Finish => {
                let Some(stream) = self.stream.take() else {
                    return "inactive".into();
                };
                if stream.pending.is_some() {
                    return self.fail("truncated");
                }
                self.store.commit(stream.staged)
            }
        }
    }
}
