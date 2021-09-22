use std::convert::TryInto;

use base64_url::{decode, encode};
use serde_json::Value;

use super::Message;
#[cfg(feature = "resolve")]
use crate::Recipient;
use crate::{
    crypto::{SignatureAlgorithm, Signer, SigningMethod, SymmetricCypherMethod},
    DidcommHeader,
    Error,
    Jwe,
    JwmHeader,
    Jws,
    MessageType,
    Signature,
};

#[derive(Serialize, Deserialize)]
pub struct PayloadToVerify {
    #[serde(flatten)]
    didcomm_header: DidcommHeader,

    #[cfg(feature = "resolve")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) recipients: Option<Vec<Recipient>>,

    body: Value,
}

#[cfg(feature = "raw-crypto")]
impl Message {
    /// Encrypts current message by consuming it.
    /// Uses provided cryptography function to perform
    ///     the encryption. Agnostic of actual algorythm used.
    /// Consuming is to make sure no changes are
    ///     possible post packaging / sending.
    /// Returns `(JwmHeader, Vec<u8>)` to be sent to receiver.
    pub fn encrypt(
        self,
        crypter: SymmetricCypherMethod,
        encryption_key: &[u8],
    ) -> Result<String, Error> {
        let mut jwe_header = self.jwm_header.clone();
        if jwe_header.typ != MessageType::DidcommForward {
            jwe_header.typ = MessageType::DidcommJwe;
        }
        let d_header = self.get_didcomm_header();
        let iv = Jwe::generate_iv();
        let multi = self.recipients.is_some();
        jwe_header.skid = Some(d_header.from.clone().unwrap_or_default());
        if !multi {
            jwe_header.kid = Some(d_header.to[0].clone());
        }
        jwe_header.skid = d_header.from.clone();
        let aad_string = encode(&serde_json::to_string(&jwe_header)?.as_bytes());
        let aad = aad_string.as_bytes();
        let cyphertext_and_tag = crypter(
            &decode(&iv)?,
            encryption_key,
            serde_json::to_string(&self)?.as_bytes(),
            aad,
        )?;
        let (cyphertext, tag) = cyphertext_and_tag.split_at(cyphertext_and_tag.len() - 16);
        let jwe;
        if self.serialize_flat_jwe {
            let recipients = self.recipients.ok_or_else(|| {
                Error::Generic("flat JWE JSON serialization needs a recipient".to_string())
            })?;
            if recipients.len() != 1 {
                return Err(Error::Generic(
                    "flat JWE JSON serialization needs exactly one recipient".to_string(),
                ));
            }

            jwe = Jwe::new_flat(
                None,
                recipients[0].clone(),
                cyphertext.to_vec(),
                Some(jwe_header),
                Some(tag),
                Some(iv),
            );
        } else {
            jwe = Jwe::new(
                None,
                self.recipients.clone(),
                cyphertext.to_vec(),
                Some(jwe_header),
                Some(tag),
                Some(iv),
            );
        }
        Ok(serde_json::to_string(&jwe)?)
    }

    /// Decrypts received cypher into instance of `Message`.
    /// Received message should be encrypted with our pub key.
    /// Returns `Ok(Message)` if decryption / deserialization
    ///     succeded. `Error` othervice.
    pub fn decrypt(
        received_message: &[u8],
        decrypter: SymmetricCypherMethod,
        key: &[u8],
    ) -> Result<Self, Error> {
        let jwe: Jwe = serde_json::from_slice(received_message)?;
        let protected = jwe
            .protected
            .as_ref()
            .ok_or_else(|| Error::Generic("jwe is missing protected header".to_string()))?;
        let aad_string = encode(&serde_json::to_string(&protected)?.as_bytes());
        let aad = aad_string.as_bytes();
        let tag = jwe
            .tag
            .as_ref()
            .ok_or_else(|| "JWE is missing tag")
            .map_err(|e| Error::Generic(e.to_string()))?;
        let mut cyphertext_and_tag: Vec<u8> = vec![];
        cyphertext_and_tag.extend(&jwe.payload());
        cyphertext_and_tag.extend(&decode(&tag)?);

        return match decrypter(jwe.get_iv().as_ref(), key, &cyphertext_and_tag, &aad) {
            Ok(raw_message_bytes) => Ok(serde_json::from_slice(&raw_message_bytes)?),
            Err(e) => {
                error!("decryption failed; {}", &e);
                Err(Error::PlugCryptoFailure)
            }
        };
    }

    /// Signs message and turns it into `Jws` envelope.
    /// `Err` is returned if message is not properly prepared or data is malformed.
    /// Jws enveloped payload is base64_url encoded
    pub fn sign(
        mut self,
        signer: SigningMethod,
        signing_sender_private_key: &[u8],
    ) -> Result<String, Error> {
        let mut jws_header = self.jwm_header.clone();
        jws_header.typ = MessageType::DidcommJws;
        if jws_header.alg.is_none() {
            return Err(Error::JwsParseError);
        }

        // drop non jwm plain message header info
        self.jwm_header = JwmHeader::default();

        let jws_header_string_base64 = base64_url::encode(&serde_json::to_string(&jws_header)?);
        let payload_json_string = serde_json::to_string(&self)?;
        let payload_string_base64 = base64_url::encode(&payload_json_string);
        let payload_to_sign = format!("{}.{}", &jws_header_string_base64, &payload_string_base64);
        let signature = signer(signing_sender_private_key, &payload_to_sign.as_bytes())?;
        let signature_value = Signature::new(Some(jws_header.clone()), None, signature);

        let jws: Jws;
        if self.serialize_flat_jws {
            jws = Jws::new_flat(payload_string_base64, signature_value);
        } else {
            let signature_values = self
                .didcomm_header
                .to
                .iter()
                .map(|_| signature_value.clone())
                .collect();
            jws = Jws::new(payload_string_base64, signature_values);
        }

        Ok(serde_json::to_string(&jws)?)
    }

    /// Verifyes signature and returns payload message on verification success.
    /// `Err` return if signature invalid or data is malformed.
    /// Expects Jws's payload to be a valid serialized `Message` and base64_url encoded.
    pub fn verify(jws: &[u8], signing_sender_public_key: &[u8]) -> Result<Message, Error> {
        let jws: Jws = serde_json::from_slice(jws)?;

        let signatures_values_to_verify: Vec<Signature>;
        if let Some(signatures) = &jws.signatures {
            signatures_values_to_verify = signatures.clone();
        } else if let Some(signature_value) = &jws.signature {
            signatures_values_to_verify = vec![signature_value.clone()];
        } else {
            return Err(Error::JwsParseError);
        }
        let payload = &jws.payload;

        let mut verified = false;
        for signature_value in signatures_values_to_verify.clone() {
            let alg = &signature_value.alg().ok_or(Error::JweParseError)?;
            let signature = &signature_value.signature[..];
            let verifyer: SignatureAlgorithm = alg.try_into()?;
            let protected_header = signature_value
                .protected
                .as_ref()
                .ok_or(Error::JwsParseError)?;
            let encoded_header = base64_url::encode(&serde_json::to_string(&protected_header)?);
            let payload_to_verify = format!("{}.{}", &encoded_header, &payload);
            if verifyer.validator()(
                signing_sender_public_key,
                &payload_to_verify.as_bytes(),
                signature,
            )? {
                verified = true;
                break;
            }
        }

        if verified {
            // body in JWS envelope should be a valid JWM message, so parse it into message
            let message: Message = serde_json::from_slice(&base64_url::decode(&jws.payload)?)?;
            Ok(message)
        } else {
            Err(Error::JwsParseError)
        }
    }

    /// Verifies signature and returns payload message on verification success.
    /// `Err` return if signature invalid or data is malformed.
    /// Expects Jws's payload to be a valid serialized `Message` and base64_url encoded.
    pub fn verify_value(jws: &Value, key: &[u8]) -> Result<Message, Error> {
        let jws_string = serde_json::to_string(jws)?;
        Message::verify(&jws_string.into_bytes(), key)
    }
}

#[cfg(test)]
mod raw_tests {
    use chacha20poly1305::{
        aead::{Aead, NewAead},
        Key,
        XChaCha20Poly1305,
        XNonce,
    };
    use sodiumoxide::crypto::secretbox;
    use x25519_dalek::{EphemeralSecret, PublicKey};

    use super::{Error, Message};
    use crate::crypto::CryptoAlgorithm;

    #[test]
    #[allow(non_snake_case)]
    #[cfg(feature = "raw-crypto")]
    fn plugin_crypto_xChaCha20Paly1305_dummy_key() {
        // Arrange
        let key = Key::from_slice(b"an example very very secret key.");
        // Plugable encryptor function to encrypt data
        let my_crypter = Box::new(
            |n: &[u8], k: &[u8], m: &[u8], _a: &[u8]| -> Result<Vec<u8>, Error> {
                let aead = XChaCha20Poly1305::new(k.into());
                let nonce = XNonce::from_slice(n);
                aead.encrypt(nonce, m)
                    .map_err(|e| Error::Generic(e.to_string()))
            },
        );
        // Plugable decryptor function to decrypt data
        let my_decrypter = Box::new(
            |n: &[u8], k: &[u8], m: &[u8], _a: &[u8]| -> Result<Vec<u8>, Error> {
                let aead = XChaCha20Poly1305::new(k.into());
                let nonce = XNonce::from_slice(n);
                Ok(aead.decrypt(nonce, m).unwrap())
            },
        );
        let m = Message::new().as_jwe(&CryptoAlgorithm::A256GCM, None);
        let id = m.get_didcomm_header().id.to_owned();

        // Act and Assert
        let crypted = m.encrypt(my_crypter, key);
        assert!(&crypted.is_ok()); // Encryption check
        let raw_m = Message::decrypt(&crypted.unwrap().as_bytes(), my_decrypter, key);
        assert!(&raw_m.is_ok()); // Decryption check
        assert_eq!(id, raw_m.unwrap().get_didcomm_header().id); // Data consistancy check
    }

    #[test]
    #[cfg(feature = "raw-crypto")]
    fn plugin_crypto_libsodium_box() {
        // Arrange
        // Plugable encryptor function to encrypt data
        let my_crypter = Box::new(
            |n: &[u8], k: &[u8], m: &[u8], _a: &[u8]| -> Result<Vec<u8>, Error> {
                let nonce = secretbox::Nonce::from_slice(n).unwrap();
                Ok(secretbox::seal(
                    m,
                    &nonce,
                    &secretbox::Key::from_slice(k).unwrap(),
                ))
            },
        );
        // Plugable decryptor function to decrypt data
        let my_decrypter = Box::new(
            |n: &[u8], k: &[u8], m: &[u8], _a: &[u8]| -> Result<Vec<u8>, Error> {
                let nonce = secretbox::Nonce::from_slice(n).unwrap();
                Ok(secretbox::open(m, &nonce, &secretbox::Key::from_slice(k).unwrap()).unwrap())
            },
        );
        let m = Message::new().as_jwe(&CryptoAlgorithm::A256GCM, None);
        let id = m.get_didcomm_header().id.to_owned();
        let key = secretbox::gen_key();

        // Act and Assert
        let crypted = m.encrypt(my_crypter, &key.as_ref());
        assert!(&crypted.is_ok()); // Encryption checkp();
        let crypted = crypted.unwrap();
        println!("{}", &crypted);
        let raw_m = Message::decrypt(&crypted.as_bytes(), my_decrypter, &key.as_ref());
        assert!(&raw_m.is_ok()); // Decryption check
        assert_eq!(id, raw_m.unwrap().get_didcomm_header().id); // Data consistancy check
    }

    #[test]
    #[allow(non_snake_case)]
    #[cfg(feature = "raw-crypto")]
    fn plugin_crypto_xChaCha20Paly1305_x25519_dalek_shared_secret() {
        // Arrange
        let sender_sk = EphemeralSecret::new(rand_core::OsRng);
        let sender_pk = PublicKey::from(&sender_sk);
        let receiver_sk = EphemeralSecret::new(rand_core::OsRng);
        let receiver_pk = PublicKey::from(&receiver_sk);
        let sender_shared = sender_sk.diffie_hellman(&receiver_pk);
        let receiver_shared = receiver_sk.diffie_hellman(&sender_pk);
        let m = Message::new().as_jwe(&CryptoAlgorithm::XC20P, None);
        let id = m.get_didcomm_header().id.to_owned();
        // Plugable encryptor function to encrypt data
        let my_crypter = Box::new(
            |n: &[u8], k: &[u8], m: &[u8], _a: &[u8]| -> Result<Vec<u8>, Error> {
                let aead = XChaCha20Poly1305::new(k.into());
                let nonce = XNonce::from_slice(n);
                aead.encrypt(nonce, m)
                    .map_err(|e| Error::Generic(e.to_string()))
            },
        );
        // Plugable decryptor function to decrypt data
        let my_decrypter = Box::new(
            |n: &[u8], k: &[u8], m: &[u8], _a: &[u8]| -> Result<Vec<u8>, Error> {
                let aead = XChaCha20Poly1305::new(k.into());
                let nonce = XNonce::from_slice(n);
                aead.decrypt(nonce, m)
                    .map_err(|e| Error::Generic(e.to_string()))
            },
        );

        // Act and Assert
        let crypted = m.encrypt(my_crypter, sender_shared.as_bytes());
        assert!(&crypted.is_ok()); // Encryption check
        let raw_m = Message::decrypt(
            &crypted.unwrap().as_bytes(),
            my_decrypter,
            receiver_shared.as_bytes(),
        );
        assert!(&raw_m.is_ok()); // Decryption check
        assert_eq!(id, raw_m.unwrap().get_didcomm_header().id); // Data consistancy check
    }
}
