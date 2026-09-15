#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interval {
    pub start: i64,
    pub end: i64,
}

impl Interval {
    pub fn new(start: i64, end: i64) -> Option<Self> {
        (start < end).then_some(Self { start, end })
    }
}
