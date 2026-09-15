#[derive(Clone)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub entries: Vec<(usize, usize, i64)>,
}
impl Matrix {
    pub fn at(&self, row: usize, col: usize) -> i64 {
        self.entries
            .iter()
            .find(|(r, c, _)| *r == row && *c == col)
            .map_or(0, |(_, _, v)| *v)
    }
}
