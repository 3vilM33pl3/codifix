#[derive(Default)]
pub struct Register {
    pub value: i64,
}

impl Register {
    pub fn add(&mut self, delta: i64) -> bool {
        if let Some(value) = self.value.checked_add(delta) {
            self.value = value;
            true
        } else {
            false
        }
    }
    pub fn scale(&mut self, numerator: i64, denominator: i64) -> bool {
        if let Some(value) = crate::scale::apply(self.value, numerator, denominator) {
            self.value = value;
            true
        } else {
            false
        }
    }
}
