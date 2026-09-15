pub fn matrix(value: &crate::sparse::Matrix) -> String {
    let body = if value.entries.is_empty() {
        "-".into()
    } else {
        value
            .entries
            .iter()
            .map(|(r, c, v)| format!("{r},{c},{v}"))
            .collect::<Vec<_>>()
            .join(";")
    };
    format!("{}x{}:{body}", value.rows, value.cols)
}
