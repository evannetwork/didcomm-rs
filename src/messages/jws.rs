// use std::convert::{ TryFrom, TryInto };
use crate::{Jwk, JwmHeader};
use crate::messages::serialization::{base64_buffer, base64_jwm_header};

macro_rules! create_getter {
    ($field_name:ident, $field_type:ident) => {
        pub fn $field_name(&self) -> Option<$field_type> {
            if let Some(header) = &self.header {
                if let Some(value) = &header.$field_name {
                    return Some(value.clone());
                }
            }
            if let Some(protected) = &self.protected {
                if let Some(value) = &protected.$field_name {
                    return Some(value.clone());
                }
            }
            None
        }
    };
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SignatureValue {
    #[serde(skip_serializing_if = "Option::is_none")]
	#[serde(with="base64_jwm_header")]
    pub protected: Option<JwmHeader>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<JwmHeader>,

	#[serde(with="base64_buffer")]
    pub signature: Vec<u8>,
}

impl SignatureValue {
    /// Creates a new `SignatureValue` that can be used in JWS `signatures` property or
    /// as top-level (flattened) property in flattened JWS JSON serialization.
    /// 
    /// # Parameters
    ///
    /// `protected` - JWM header protected by signing
    ///
    /// `header` - JWM header not protected by signing
    ///
    /// `signature` - signature over JWS payload and protected header
    pub fn new(
        protected: Option<JwmHeader>,
        header: Option<JwmHeader>,
        signature: Vec<u8>,
    ) -> Self {
        SignatureValue {
            protected,
            header,
            signature,
        }
    }

    create_getter!(enc, String);
    create_getter!(kid, String);
    create_getter!(skid, String);
    create_getter!(alg, String);
    create_getter!(jku, String);
    create_getter!(jwk, Jwk);
    create_getter!(epk, Jwk);
    create_getter!(cty, String);
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Jws {
    pub payload: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signatures: Option<Vec<SignatureValue>>,
    #[serde(flatten)]
    pub signature_value: Option<SignatureValue>,
}

impl Jws {
    /// Creates a new [general JWS](https://datatracker.ietf.org/doc/html/rfc7515#section-7.2.1)
    /// object with signature values per recipient.
    /// 
    /// # Parameters
    ///
    /// `payload` - payload with encoded data
    ///
    /// `signatures` - signature values per recipient
    pub fn new(
        payload: String,
        signatures: Vec<SignatureValue>,
    ) -> Self {
        Jws {
            payload,
            signature_value: None,
            signatures: Some(signatures),
        }
    }

    /// Creates a new [flattened JWS](https://datatracker.ietf.org/doc/html/rfc7515#section-7.2.2)
    /// object with signature information on JWS' top level.
    /// 
    /// # Parameters
    ///
    /// `payload` - payload with encoded data
    ///
    /// `signatures` - signature value that is used on JWS top-level
    pub fn new_flat(
        payload: String,
        signature_value: SignatureValue,
    ) -> Self {
        Jws {
            payload,
            signature_value: Some(signature_value),
            signatures: None,
        }
    }
}
