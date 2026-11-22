use serde::de::IgnoredAny;
use serde::{Deserialize, Deserializer};

#[derive(Deserialize)]
#[serde(untagged)]
enum Entry<T> {
    Recognised(T),
    Unrecognised(IgnoredAny),
}

impl<T> Entry<T> {
    fn into_recognised(self) -> Option<T> {
        match self {
            Entry::Recognised(value) => Some(value),
            Entry::Unrecognised(_) => None,
        }
    }
}

pub(crate) fn recognised<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Entry::deserialize(deserializer).map(Entry::into_recognised)
}

pub(crate) fn recognised_entries<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    let entries = Vec::<Entry<T>>::deserialize(deserializer)?;
    Ok(entries
        .into_iter()
        .filter_map(Entry::into_recognised)
        .collect())
}

pub(crate) fn recognised_optional<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Entry<T>>::deserialize(deserializer)?.map(Entry::into_recognised))
}
