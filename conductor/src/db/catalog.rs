use crate::{
    crypto::keys::{Fingerprint, FingerprintRef},
    env::{locate_path, PathAsset},
    proto::TunnelRole,
    Error, Result,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
};

type TunnelId = uuid::Uuid;

trait TunnelIdExt {
    fn generate() -> Self;
}

impl TunnelIdExt for TunnelId {
    fn generate() -> Self {
        Self::new_v4()
    }
}

#[derive(Debug, Serialize, Deserialize)]
enum PeerRules {
    Open,
    BlockList(HashSet<Fingerprint>),
    AcceptList(HashSet<Fingerprint>),
}

impl PeerRules {
    fn peer_allowed(&self, peer_fingerprint: FingerprintRef) -> bool {
        match self {
            PeerRules::Open => true,
            PeerRules::AcceptList(peers) => peers.contains(peer_fingerprint),
            PeerRules::BlockList(peers) => !peers.contains(peer_fingerprint),
        }
    }

    fn make_open(&mut self) {
        match self {
            PeerRules::BlockList(_) | PeerRules::Open => {}
            PeerRules::AcceptList(_) => *self = PeerRules::Open,
        }
    }

    fn make_protected(&mut self) {
        match self {
            PeerRules::AcceptList(_) => {}
            PeerRules::BlockList(_) | PeerRules::Open => {
                *self = PeerRules::AcceptList(HashSet::new());
            }
        }
    }

    fn is_protected(&self) -> bool {
        matches!(self, PeerRules::AcceptList(_))
    }

    fn allow_peer(&mut self, peer_fingerprint: FingerprintRef) {
        match self {
            PeerRules::Open => {}
            PeerRules::AcceptList(peers) => {
                peers.insert(peer_fingerprint.to_owned());
            }
            PeerRules::BlockList(peers) => {
                peers.remove(peer_fingerprint);
            }
        }
    }

    fn block_peer(&mut self, peer_fingerprint: FingerprintRef) {
        match self {
            PeerRules::Open => {
                let mut peers = HashSet::new();
                peers.insert(peer_fingerprint.to_owned());
                *self = PeerRules::BlockList(peers);
            }
            PeerRules::AcceptList(peers) => {
                peers.remove(peer_fingerprint);
            }
            PeerRules::BlockList(peers) => {
                peers.insert(peer_fingerprint.to_owned());
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct TunnelRules {
    accept: PeerRules,
    connect: PeerRules,
}

#[derive(Debug, Serialize, Deserialize)]
struct TunnelInfo {
    name: String,
    rules: TunnelRules,
}

impl TunnelInfo {
    fn rules_for_role(&self, role: &TunnelRole) -> &PeerRules {
        match role {
            TunnelRole::Connect => &self.rules.connect,
            TunnelRole::Accept | TunnelRole::Listen => &self.rules.accept,
        }
    }

    fn rules_for_role_mut(&mut self, role: &TunnelRole) -> &mut PeerRules {
        match role {
            TunnelRole::Connect => &mut self.rules.connect,
            TunnelRole::Accept | TunnelRole::Listen => &mut self.rules.accept,
        }
    }
}

pub enum TunnelSelector {
    ById(TunnelId),
    ByAlias(String),
}

impl From<String> for TunnelSelector {
    fn from(s: String) -> Self {
        Self::ByAlias(s)
    }
}

impl From<TunnelId> for TunnelSelector {
    fn from(id: TunnelId) -> Self {
        Self::ById(id)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Catalog {
    tunnels: HashMap<TunnelId, TunnelInfo>,
    tunnel_aliases: HashMap<String, TunnelId>,
}

impl Catalog {
    pub async fn load() -> Result<Self> {
        let catalog_path = locate_path(PathAsset::PeerCatalog);

        if !catalog_path.exists() {
            return Err(Error::PeerCatalogIsMissing);
        }

        let mut catalog_file = File::open(catalog_path)
            .await
            .map_err(Error::from_io_error)?;

        let mut catalog_data = Vec::new();
        catalog_file
            .read_to_end(&mut catalog_data)
            .await
            .map_err(Error::from_io_error)?;

        let catalog =
            serde_json::from_slice(&catalog_data).map_err(|_| Error::PeerCatalogIsCorrupted)?;

        Ok(catalog)
    }

    pub async fn save(&self) -> Result<()> {
        let catalog_path = locate_path(PathAsset::PeerCatalog);

        let mut catalog_file = File::create(catalog_path)
            .await
            .map_err(Error::from_io_error)?;

        let catalog_data = serde_json::to_vec_pretty(self)
            .map_err(|_| Error::from_application_error("Catalog serialization failed"))?;

        catalog_file
            .write_all(&catalog_data)
            .await
            .map_err(Error::from_io_error)?;

        Ok(())
    }

    pub fn is_tunnel_access_granted_for_peer(
        &self,
        tunnel: &TunnelSelector,
        role: TunnelRole,
        fingerprint: FingerprintRef,
    ) -> Result<bool> {
        Ok(self
            .get_tunnel_info(tunnel)?
            .rules_for_role(&role)
            .peer_allowed(fingerprint))
    }

    pub fn grant_tunnel_access_for_peer(
        &mut self,
        tunnel: &TunnelSelector,
        role: TunnelRole,
        fingerprint: FingerprintRef,
    ) -> Result<()> {
        self.get_tunnel_mut(tunnel)?
            .rules_for_role_mut(&role)
            .allow_peer(fingerprint);
        Ok(())
    }

    pub fn block_tunnel_access_for_peer(
        &mut self,
        tunnel: &TunnelSelector,
        role: TunnelRole,
        fingerprint: FingerprintRef,
    ) -> Result<()> {
        self.get_tunnel_mut(tunnel)?
            .rules_for_role_mut(&role)
            .block_peer(fingerprint);
        Ok(())
    }

    pub fn is_tunnel_protected(&self, tunnel: &TunnelSelector) -> Result<bool> {
        Ok(self
            .get_tunnel_info(tunnel)?
            .rules_for_role(&TunnelRole::Connect)
            .is_protected())
    }

    pub fn make_tunnel_open(&mut self, tunnel: &TunnelSelector) -> Result<()> {
        self.get_tunnel_mut(tunnel)?
            .rules_for_role_mut(&TunnelRole::Connect)
            .make_open();
        Ok(())
    }

    pub fn make_tunnel_protected(&mut self, tunnel: &TunnelSelector) -> Result<()> {
        self.get_tunnel_mut(tunnel)?
            .rules_for_role_mut(&TunnelRole::Connect)
            .make_protected();
        Ok(())
    }

    pub fn create_tunnel(&mut self, name: &str) -> Result<TunnelId> {
        let id = TunnelId::generate();

        if self.tunnel_aliases.contains_key(name) || self.tunnels.contains_key(&id) {
            return Err(Error::TunnelAlreadyExist);
        }

        self.tunnel_aliases.insert(name.to_owned(), id);
        self.tunnels.insert(
            id,
            TunnelInfo {
                name: name.to_owned(),
                rules: TunnelRules {
                    // By default, any peer will be rejected
                    accept: PeerRules::AcceptList(HashSet::new()),
                    connect: PeerRules::AcceptList(HashSet::new()),
                },
            },
        );

        Ok(id)
    }

    pub fn remove_tunnel(&mut self, tunnel: &TunnelSelector) -> Result<()> {
        let (id, name) = match tunnel {
            TunnelSelector::ById(id) => {
                let name = self
                    .tunnels
                    .get(id)
                    .ok_or(Error::TunnelDoesNotExist)?
                    .name
                    .clone();
                (*id, name)
            }
            TunnelSelector::ByAlias(alias) => {
                let id = self
                    .tunnel_aliases
                    .get(alias)
                    .ok_or(Error::TunnelAliasDoesNotExist)?;
                (*id, alias.clone())
            }
        };

        self.tunnel_aliases.remove(&name);
        self.tunnels.remove(&id);
        Ok(())
    }

    fn get_tunnel_info(&self, tunnel: &TunnelSelector) -> Result<&TunnelInfo> {
        let id = match tunnel {
            TunnelSelector::ById(id) => id,
            TunnelSelector::ByAlias(alias) => self
                .tunnel_aliases
                .get(alias)
                .ok_or(Error::TunnelAliasDoesNotExist)?,
        };
        self.tunnels.get(id).ok_or(Error::CorruptedCatalog)
    }

    fn get_tunnel_mut(&mut self, tunnel: &TunnelSelector) -> Result<&mut TunnelInfo> {
        let Self {
            tunnels,
            tunnel_aliases,
        } = self;

        let id = match tunnel {
            TunnelSelector::ById(id) => id,
            TunnelSelector::ByAlias(alias) => tunnel_aliases
                .get(alias)
                .ok_or(Error::TunnelAliasDoesNotExist)?,
        };
        tunnels.get_mut(id).ok_or(Error::CorruptedCatalog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn serialization() {
        let mut catalog = Catalog::default();
        let _tunnel1_id = catalog.create_tunnel("foo").unwrap();
        let tunnel2_id = catalog.create_tunnel("bar").unwrap();
        let _tunnel3_id = catalog.create_tunnel("foobar").unwrap();

        catalog.make_tunnel_open(&"foo".to_string().into()).unwrap();
        catalog.make_tunnel_protected(&tunnel2_id.into()).unwrap();
        catalog
            .make_tunnel_open(&"foobar".to_string().into())
            .unwrap();

        catalog
            .block_tunnel_access_for_peer(
                &"foo".to_string().into(),
                TunnelRole::Connect,
                &"f_o_o".to_owned(),
            )
            .unwrap();
        catalog
            .grant_tunnel_access_for_peer(
                &"bar".to_string().into(),
                TunnelRole::Accept,
                &"b_a_r".to_owned(),
            )
            .unwrap();

        expect![[r#"
            {
              "tunnels": {
                "cf702cc4-b1aa-48b9-9252-b69e090e1c39": {
                  "name": "foo",
                  "rules": {
                    "accept": {
                      "AcceptList": []
                    },
                    "connect": {
                      "BlockList": [
                        "f_o_o"
                      ]
                    }
                  }
                },
                "47aefe69-d0aa-44f5-937e-1dd4e23eb36f": {
                  "name": "foobar",
                  "rules": {
                    "accept": {
                      "AcceptList": []
                    },
                    "connect": "Open"
                  }
                },
                "70f7046e-260a-4e0d-9539-85fe5f6f86c4": {
                  "name": "bar",
                  "rules": {
                    "accept": {
                      "AcceptList": [
                        "b_a_r"
                      ]
                    },
                    "connect": {
                      "AcceptList": []
                    }
                  }
                }
              },
              "tunnel_aliases": {
                "foobar": "47aefe69-d0aa-44f5-937e-1dd4e23eb36f",
                "foo": "cf702cc4-b1aa-48b9-9252-b69e090e1c39",
                "bar": "70f7046e-260a-4e0d-9539-85fe5f6f86c4"
              }
            }"#]]
        .assert_eq(&serde_json::to_string_pretty(&catalog).unwrap())
    }
}
