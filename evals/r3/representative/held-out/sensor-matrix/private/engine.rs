use crate::{command::Command, sparse::Matrix};
use std::collections::BTreeMap;
#[derive(Default)]
pub struct Engine {
    matrices: BTreeMap<String, Matrix>,
    revision: u64,
}
fn canonical(matrix: Matrix) -> Result<Matrix, &'static str> {
    if matrix.rows == 0 || matrix.cols == 0 || matrix.rows > 8 || matrix.cols > 8 {
        return Err("shape");
    }
    let mut values = BTreeMap::<(usize, usize), i128>::new();
    for (r, c, v) in matrix.entries {
        if r >= matrix.rows || c >= matrix.cols {
            return Err("shape");
        }
        let entry = values.entry((r, c)).or_default();
        *entry = entry.checked_add(i128::from(v)).ok_or("overflow")?;
    }
    let mut entries = Vec::new();
    for ((r, c), v) in values {
        let value = i64::try_from(v).map_err(|_| "overflow")?;
        if value != 0 {
            entries.push((r, c, value));
        }
    }
    Ok(Matrix {
        rows: matrix.rows,
        cols: matrix.cols,
        entries,
    })
}
impl Engine {
    fn put(&mut self, id: String, matrix: Matrix) -> String {
        let matrix = match canonical(matrix) {
            Ok(value) => value,
            Err(error) => return error.into(),
        };
        let Some(next) = self.revision.checked_add(1) else {
            return "overflow".into();
        };
        self.matrices.insert(id, matrix);
        self.revision = next;
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
                std::mem::swap(&mut matrix.rows, &mut matrix.cols);
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
                if a.cols != b.rows {
                    return "shape".into();
                }
                let mut result = Matrix {
                    rows: a.rows,
                    cols: b.cols,
                    entries: Vec::new(),
                };
                for r in 0..a.rows {
                    for c in 0..b.cols {
                        let mut sum = 0i128;
                        for k in 0..a.cols {
                            let Some(product) =
                                i128::from(a.at(r, k)).checked_mul(i128::from(b.at(k, c)))
                            else {
                                return "overflow".into();
                            };
                            let Some(next) = sum.checked_add(product) else {
                                return "overflow".into();
                            };
                            sum = next;
                        }
                        let Ok(value) = i64::try_from(sum) else {
                            return "overflow".into();
                        };
                        if value != 0 {
                            result.entries.push((r, c, value));
                        }
                    }
                }
                self.put(dst, result)
            }
        }
    }
}
