use crate::capture::Capture;
#[derive(Clone)]
pub struct Snapshot {
    pub version: usize,
    pub author: String,
    pub body: String,
    pub editor: String,
}
#[derive(Clone)]
pub struct Review {
    pub publisher: String,
    pub snapshot: Snapshot,
}
pub fn prepare(capture: &Capture, publisher: &str, version: usize) -> Review {
    Review {
        publisher: publisher.into(),
        snapshot: Snapshot {
            version,
            author: capture.author.clone(),
            body: capture.body.clone(),
            editor: "-".into(),
        },
    }
}
