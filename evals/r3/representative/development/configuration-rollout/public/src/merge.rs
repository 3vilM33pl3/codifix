use std::collections::BTreeMap;
pub fn merge(
    base: &BTreeMap<String, String>,
    overlay: &BTreeMap<String, Option<String>>,
) -> BTreeMap<String, String> {
    let mut result: BTreeMap<_, _> = overlay
        .iter()
        .filter_map(|(k, v)| v.clone().map(|v| (k.clone(), v)))
        .collect();
    for (key, value) in base {
        result.insert(key.clone(), value.clone());
    }
    result
}
