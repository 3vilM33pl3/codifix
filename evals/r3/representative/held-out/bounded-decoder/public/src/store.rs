#[derive(Default)]
pub struct Store {
    pub bytes: Vec<u8>,
    pub generation: u64,
}
impl Store {
    pub fn commit(&mut self, bytes: Vec<u8>) -> String {
        self.bytes = bytes;
        self.generation += 1;
        format!("committed:{}", self.generation)
    }
    pub fn show(&self) -> String {
        format!("{}:{}", self.generation, crate::hex::encode(&self.bytes))
    }
}
