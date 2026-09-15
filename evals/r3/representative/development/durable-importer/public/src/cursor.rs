#[derive(Clone, Default)]
pub struct Cursor(pub u64);

impl Cursor {
    pub fn accepts(&self, next: u64) -> bool {
        self.0.checked_add(1) == Some(next)
    }
}
