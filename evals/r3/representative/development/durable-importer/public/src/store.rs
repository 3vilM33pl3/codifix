use crate::{cursor::Cursor, input::Delivery};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub struct Store {
    pub cursor: Cursor,
    pub total: i64,
    pub accepted: BTreeMap<u64, Delivery>,
    pub pending: BTreeMap<u64, Delivery>,
}

impl Store {
    pub fn snapshot(&self) -> String {
        format!(
            "cursor={} total={} accepted={} pending={}",
            self.cursor.0,
            self.total,
            self.accepted.len(),
            self.pending.len()
        )
    }
}
