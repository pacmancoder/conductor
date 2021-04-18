use serde::{Deserialize, Serialize};

#[derive(Debug, Default)]
pub struct Base64EncodedData(Vec<u8>);

impl<T> From<T> for Base64EncodedData
where
    T: Into<Vec<u8>>,
{
    fn from(data: T) -> Self {
        Self(data.into())
    }
}

impl Serialize for Base64EncodedData {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let encoded = base64::encode(&self.0);
        encoded.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Base64EncodedData {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;

        let encoded = String::deserialize(deserializer)?;
        let decoded =
            base64::decode(encoded).map_err(|_| D::Error::custom("Invalid base64 value"))?;

        Ok(Base64EncodedData::from(decoded))
    }
}
