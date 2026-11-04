//! Deserialization of the `&'static str` fields of the stem evidence, for a
//! plan in two invocations (D263). Each distinct text is allocated once for
//! the life of the process.
use serde::{Deserialize, Deserializer};
use std::{collections::BTreeSet, sync::Mutex};

static TEXTS: Mutex<BTreeSet<&'static str>> = Mutex::new(BTreeSet::new());

fn intern(text: String) -> &'static str {
    let mut texts = TEXTS.lock().unwrap_or_else(|error| error.into_inner());
    if let Some(&kept) = texts.get(text.as_str()) {
        return kept;
    }
    let kept: &'static str = Box::leak(text.into_boxed_str());
    texts.insert(kept);
    kept
}

pub(crate) fn text<'de, D: Deserializer<'de>>(input: D) -> Result<&'static str, D::Error> {
    String::deserialize(input).map(intern)
}

pub(crate) fn texts<'de, D: Deserializer<'de>>(input: D) -> Result<Vec<&'static str>, D::Error> {
    Vec::<String>::deserialize(input).map(|texts| texts.into_iter().map(intern).collect())
}

pub(crate) fn four<'de, D: Deserializer<'de>>(input: D) -> Result<[&'static str; 4], D::Error> {
    <[String; 4]>::deserialize(input).map(|texts| texts.map(intern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_is_allocated_once() {
        let first = intern("set.static-text.test".to_string());
        let second = intern("set.static-text.test".to_string());
        assert_eq!(first, "set.static-text.test");
        assert!(std::ptr::eq(first, second));
    }
}
