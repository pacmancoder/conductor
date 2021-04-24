#[derive(Debug, Copy, Clone)]
pub enum ProtocolRevision {
    R0,
    Unknown(u32),
}

impl From<u32> for ProtocolRevision {
    fn from(rev: u32) -> Self {
        match rev {
            0 => Self::R0,
            rev => Self::Unknown(rev),
        }
    }
}

impl From<ProtocolRevision> for u32 {
    fn from(rev: ProtocolRevision) -> Self {
        match rev {
            ProtocolRevision::R0 => 0,
            ProtocolRevision::Unknown(rev) => rev,
        }
    }
}

impl std::fmt::Display for ProtocolRevision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let numeric_value = u32::from(*self);
        write!(f, "r{}", numeric_value)
    }
}

impl serde::Serialize for ProtocolRevision {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let numeric_value = u32::from(*self);
        serializer.serialize_u32(numeric_value)
    }
}

impl<'de> serde::Deserialize<'de> for ProtocolRevision {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let numeric_value = u32::deserialize(deserializer)?;
        Ok(numeric_value.into())
    }
}
