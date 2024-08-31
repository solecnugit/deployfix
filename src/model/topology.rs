use serde::{Deserialize, Serialize};

pub static METADATA_TOPOLOGY_KEY: &str = "topology";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityRuleTopologyKey {
    Zone,
    Rack,
    Node,
}

impl From<&str> for EntityRuleTopologyKey {
    fn from(s: &str) -> Self {
        match s {
            "zone" => Self::Zone,
            "rack" => Self::Rack,
            "node" => Self::Node,
            _ => panic!("Unknown topology key: {}", s),
        }
    }
}

impl AsRef<str> for EntityRuleTopologyKey {
    fn as_ref(&self) -> &str {
        match self {
            Self::Zone => "zone",
            Self::Rack => "rack",
            Self::Node => "node",
        }
    }
}

impl ToString for EntityRuleTopologyKey {
    fn to_string(&self) -> String {
        self.as_ref().to_string()
    }
}
