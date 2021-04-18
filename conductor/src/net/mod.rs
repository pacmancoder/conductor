use crate::error::ConductorError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone)]
pub struct PortNum(u16);

impl FromStr for PortNum {
    type Err = ConductorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.parse::<u16>()
            .map_err(|_| ConductorError::from_string_parsing_error(s, "Invalid port number"))
            .map(|p| Self(p))
    }
}

#[derive(Serialize, Deserialize)]
pub enum Protocol {
    Tcp,
    Udp,
}

impl FromStr for Protocol {
    type Err = ConductorError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tcp" => Ok(Self::Tcp),
            "udp" => Ok(Self::Udp),
            protocol @ _ => Err(ConductorError::from_string_parsing_error(
                protocol,
                "Invalid protocol",
            )),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum TunnelTransport {
    Raw,
    Tls,
}
