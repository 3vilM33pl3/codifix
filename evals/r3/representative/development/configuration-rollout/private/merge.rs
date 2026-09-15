use std::collections::BTreeMap;
pub fn merge(
    base: &BTreeMap<String, String>,
    overlay: &BTreeMap<String, Option<String>>,
) -> BTreeMap<String, String> {
    let mut result = base.clone();
    for (key, value) in overlay {
        match value {
            Some(value) => {
                result.insert(key.clone(), value.clone());
            }
            None => {
                result.remove(key);
            }
        }
    }
    result
}
