use std::{collections::HashMap, time::SystemTime};

use rand::Rng;

use crate::{Error, PriorClaims};

/// Collection of DIDComm message specific headers, will be flattened into DIDComm plain message
/// according to [spec](https://datatracker.ietf.org/doc/html/draft-looker-jwm-01#section-4).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DidCommHeader {
    pub id: String,

    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub to: Vec<String>,

    pub from: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_time: Option<u64>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_time: Option<u64>,

    #[serde(flatten)]
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub(crate) other: HashMap<String, String>,

    /// A JWT, with sub: new DID and iss: prior DID,
    /// with a signature from a key authorized by prior DID.
    #[serde(skip_serializing_if = "Option::is_none")]
    from_prior: Option<PriorClaims>,
}

impl DidCommHeader {
    /// Constructor function with ~default values.
    pub fn new() -> Self {
        DidCommHeader {
            id: DidCommHeader::gen_random_id(),
            to: vec![String::default()],
            from: Some(String::default()),
            created_time: None,
            expires_time: None,
            from_prior: None,
            other: HashMap::new(),
        }
    }

    /// Generates random `id`
    /// TODO: Should this be public?
    pub fn gen_random_id() -> String {
        let id_number: usize = rand::thread_rng().gen();
        format!("{}", id_number)
    }

    /// Getter method for `from_prior` retrieval
    pub fn from_prior(&self) -> &Option<PriorClaims> {
        &self.from_prior
    }

    /// Creates set of DIDComm related headers with the static forward type
    pub fn forward(
        to: Vec<String>,
        from: Option<String>,
        expires_time: Option<u64>,
    ) -> Result<Self, Error> {
        Ok(DidCommHeader {
            id: DidCommHeader::gen_random_id(),
            to,
            from,
            created_time: Some(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)?
                    .as_secs(),
            ),
            expires_time,
            ..DidCommHeader::new()
        })
    }
}

impl Default for DidCommHeader {
    fn default() -> Self {
        DidCommHeader::new()
    }
}