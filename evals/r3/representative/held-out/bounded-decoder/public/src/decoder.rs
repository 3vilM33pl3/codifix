use crate::{command::Command, store::Store};
struct Stream {
    limit: usize,
    encoded: usize,
    staged: Vec<u8>,
}
#[derive(Default)]
pub struct Decoder {
    store: Store,
    stream: Option<Stream>,
}
impl Decoder {
    pub fn run(&mut self, command: Command) -> String {
        match command {
            Command::Begin(limit) => {
                self.stream = Some(Stream {
                    limit,
                    encoded: 0,
                    staged: Vec::new(),
                });
                "ready".into()
            }
            Command::Show => self.store.show(),
            Command::Status => self
                .stream
                .as_ref()
                .map(|s| format!("active in:{} out:{} pending:no", s.encoded, s.staged.len()))
                .unwrap_or_else(|| "idle".into()),
            Command::Feed(bytes) => {
                let Some(stream) = self.stream.as_mut() else {
                    return "inactive".into();
                };
                let _ = stream.limit;
                stream.encoded += bytes.len();
                for pair in bytes.chunks_exact(2) {
                    stream
                        .staged
                        .extend(std::iter::repeat_n(pair[1], usize::from(pair[0])));
                }
                self.store.bytes = stream.staged.clone();
                "queued".into()
            }
            Command::Finish => {
                let Some(stream) = self.stream.take() else {
                    return "inactive".into();
                };
                self.store.commit(stream.staged)
            }
        }
    }
}
