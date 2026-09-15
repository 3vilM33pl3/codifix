#[derive(Clone)]
pub struct License {
    pub source: bool,
    pub text: String,
}
#[derive(Clone)]
pub struct Component {
    pub license: String,
    pub notice: String,
    pub offer: Option<String>,
}
#[derive(Default)]
pub struct Registry {
    pub licenses: std::collections::BTreeMap<String, License>,
    pub components: std::collections::BTreeMap<String, Component>,
    pub selected: std::collections::BTreeSet<String>,
}
