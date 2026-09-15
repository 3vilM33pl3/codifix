use crate::{command::Command, sparse::Matrix};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Engine {
    matrices: BTreeMap<String, Matrix>,
    revision: u64,
}
impl Engine {
    fn put(&mut self, id: String, mut matrix: Matrix) -> String {
        if matrix.rows == 0 || matrix.cols == 0 || matrix.rows > 8 || matrix.cols > 8 {
            return "shape".into();
        }
        matrix.entries.sort_by_key(|(r, c, _)| (*r, *c));
        self.matrices.insert(id, matrix);
        self.revision += 1;
        "ok".into()
    }
    pub fn run(&mut self, command: Command) -> String {
        match command {
            Command::Put(id, matrix) => self.put(id, matrix),
            Command::Revision => self.revision.to_string(),
            Command::Show(id) => self
                .matrices
                .get(&id)
                .map(crate::display::matrix)
                .unwrap_or_else(|| "missing".into()),
            Command::Transpose(dst, src) => {
                let Some(mut matrix) = self.matrices.get(&src).cloned() else {
                    return "missing".into();
                };
                for (r, c, _) in &mut matrix.entries {
                    std::mem::swap(r, c);
                }
                self.put(dst, matrix)
            }
            Command::Mul(dst, left, right) => {
                let (Some(a), Some(b)) = (self.matrices.get(&left), self.matrices.get(&right))
                else {
                    return "missing".into();
                };
                let mut result = Matrix {
                    rows: a.rows,
                    cols: b.cols,
                    entries: Vec::new(),
                };
                for r in 0..a.rows {
                    for c in 0..b.cols {
                        let v = a.at(r, c).wrapping_mul(b.at(r, c));
                        if v != 0 {
                            result.entries.push((r, c, v));
                        }
                    }
                }
                self.put(dst, result)
            }
        }
    }
}
