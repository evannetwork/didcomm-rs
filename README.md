# didcomm-rs

Rust implementation of DIDComm v2 [spec](https://identity.foundation/didcomm-messaging/spec)

![tests](https://github.com/decentralized-identity/didcomm-rs/workflows/tests/badge.svg)

## License

[Apache-2.0](LICENSE.md)

## Examples of usage

### 1. Prepare raw message for send and receive

#### GoTo: [full test][send_receive_raw]

```rust
// Message construction
let m = Message::new()
    // setting `from` header (sender) - Optional
    .from("did:xyz:ulapcuhsatnpuhza930hpu34n_")
    // setting `to` header (recipients) - Optional
    .to(&[
        "did::xyz:34r3cu403hnth03r49g03",
        "did:xyz:30489jnutnjqhiu0uh540u8hunoe",
    ])
    // populating body with some data - `Vec<bytes>`
    .body(TEST_DID).unwrap();

// Serialize message into JWM json (SENDER action)
let ready_to_send = m.clone().as_raw_json().unwrap();

// ... transport is happening here ...

// On receival deserialize from json into Message (RECEIVER action)
// Error handling recommended here

let received = Message::receive(&ready_to_send, None, None, None);
```

### 2. Prepare JWE message for direct send

#### GoTo: [full test][send_receive_encrypted_xc20p_json_test]

```rust
// sender key as bytes
let ek = [130, 110, 93, 113, 105, 127, 4, 210, 65, 234, 112, 90, 150, 120, 189, 252, 212, 165, 30, 209, 194, 213, 81, 38, 250, 187, 216, 14, 246, 250, 166, 92];

// Message construction
let message = Message::new()
    .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
    .to(&[
        "did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp",
        "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG",
    ])
    // packing in some payload (can be anything really)
    .body(TEST_DID).unwrap()
    // decide which [Algorithm](crypto::encryptor::CryptoAlgorithm) is used (based on key)
    .as_jwe(
        &CryptoAlgorithm::XC20P,
        Some(&bobs_public),
    )
    // add some custom app/protocol related headers to didcomm header portion
    // these are not included into JOSE header
    .add_header_field("my_custom_key".into(), "my_custom_value".into())
    .add_header_field("another_key".into(), "another_value".into())
    // set `kid` property
    .kid(r#"#z6LShs9GGnqk85isEBzzshkuVWrVKsRp24GnDuHk8QWkARMW"#);

// recipient public key is automatically resolved
let ready_to_send = message.seal(
    &ek,
    Some(vec![Some(&bobs_public), Some(&carol_public)]),
).unwrap();

//... transport is happening here ...
```

### 3. Prepare JWS message -> send -> receive

* Here `Message` is signed but not encrypted.
* In such scenarios explicit use of `.sign(...)` and `Message::verify(...)` required.

```rust
// Message construction an JWS wrapping
let message = Message::new() // creating message
    .from("did:xyz:ulapcuhsatnpuhza930hpu34n_") // setting from
    .to(&["did::xyz:34r3cu403hnth03r49g03", "did:xyz:30489jnutnjqhiu0uh540u8hunoe"]) // setting to
    .body(TEST_DID).unwrap() // packing in some payload
    .as_jws(&SignatureAlgorithm::EdDsa)
    .sign(SignatureAlgorithm::EdDsa.signer(), &sign_keypair.to_bytes()).unwrap();

//... transport is happening here ...

// Receiving JWS
let received = Message::verify(&message.as_bytes(), &sign_keypair.public.to_bytes());
```

### 4. Prepare JWE message to be mediated -> mediate -> receive

* Message should be encrypted by destination key first in `.routed_by()` method call using key for the recipient.
* Next it should be encrypted by mediator key in `.seal()` method call - this can be done multiple times - once for each mediator in chain but should be strictly sequential to match mediators sequence in the chain.
* Method call `.seal()` **MUST** be preceded by  `.as_jwe(CryptoAlgorithm)` as mediators may use different algorithms and key types than destination and this is not automatically predicted or populated.
* Keys used for encryption should be used in reverse order - final destination - last mediator - second to last mediator - etc. Onion style.

#### GoTo: [full test][send_receive_mediated_encrypted_xc20p_json_test]

```rust
let mediated = Message::new()
    // setting from
    .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
    // setting to
    .to(&["did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG"])
    // packing in some payload
    .body(r#"{"foo":"bar"}"#).unwrap()
    // set JOSE header for XC20P algorithm
    .as_jwe(&CryptoAlgorithm::XC20P, Some(&bobs_public))
    // custom header
    .add_header_field("my_custom_key".into(), "my_custom_value".into())
    // another custom header
    .add_header_field("another_key".into(), "another_value".into())
    // set kid header
    .kid(&"Ef1sFuyOozYm3CEY4iCdwqxiSyXZ5Br-eUDdQXk6jaQ")
    // here we use destination key to bob and `to` header of mediator -
    //**THIS MUST BE LAST IN THE CHAIN** - after this call you'll get new instance of envelope `Message` destined to the mediator.
    .routed_by(
        &alice_private,
        Some(vec![Some(&bobs_public)]),
        "did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf",
        Some(&mediators_public),
    );
assert!(mediated.is_ok());

//... transport to mediator is happening here ...

// Received by mediator
let mediator_received = Message::receive(
    &mediated.unwrap(),
    Some(&mediators_private),
    Some(&alice_public),
    None,
);
assert!(mediator_received.is_ok());

// Get inner JWE as string from message
let mediator_received_unwrapped = mediator_received.unwrap().get_body().unwrap();
let pl_string = String::from_utf8_lossy(mediator_received_unwrapped.as_ref());
let message_to_forward: Mediated = serde_json::from_str(&pl_string).unwrap();
let attached_jwe = serde_json::from_slice::<Jwe>(&message_to_forward.payload);
assert!(attached_jwe.is_ok());
let str_jwe = serde_json::to_string(&attached_jwe.unwrap());
assert!(str_jwe.is_ok());

//... transport to destination is happening here ...

// Received by Bob
let bob_received = Message::receive(
    &String::from_utf8_lossy(&message_to_forward.payload),
    Some(&bobs_private),
    Some(&alice_public),
    None,
);
assert!(bob_received.is_ok());
```

### 5. Prepare JWS envelope wrapped into JWE -> sign -> pack -> receive

* JWS header is set automatically based on signing algorithm type.
* Message forming and encryption happens in same way as in other JWE examples.
* ED25519-dalek signature is used in this example with keypair for signing and public key for verification.

#### GoTo: [full test][send_receive_direct_signed_and_encrypted_xc20p_test]

```rust
let KeyPairSet {
    alice_public,
    alice_private,
    bobs_private,
    bobs_public,
    ..
} = get_keypair_set();
// Message construction
let message = Message::new() // creating message
    .from("did:xyz:ulapcuhsatnpuhza930hpu34n_") // setting from
    .to(&["did::xyz:34r3cu403hnth03r49g03"]) // setting to
    .body(TEST_DID).unwrap() // packing in some payload
    .as_jwe(&CryptoAlgorithm::XC20P, Some(&bobs_public)) // set JOSE header for XC20P algorithm
    .add_header_field("my_custom_key".into(), "my_custom_value".into()) // custom header
    .add_header_field("another_key".into(), "another_value".into()) // another custom header
    .kid(r#"Ef1sFuyOozYm3CEY4iCdwqxiSyXZ5Br-eUDdQXk6jaQ"#); // set kid header

// Send as signed and encrypted JWS wrapped into JWE
let ready_to_send = message.seal_signed(
    &alice_private,
    Some(vec![Some(&bobs_public)]),
    SignatureAlgorithm::EdDsa,
    &sign_keypair.to_bytes(),
).unwrap();

//... transport to destination is happening here ...

// Receive - same method to receive for JWE or JWS wrapped into JWE but with pub verifying key
let received = Message::receive(
    &ready_to_send,
    Some(&bobs_private),
    Some(&alice_public),
    None,
); // and now we parse received
```

### 6. Multiple recipients static key wrap per recipient with shared secret

* ! Works with `resolve` feature only - requires resolution of public keys for each recipient for shared secret generation.
* Static key generated randomly in the background (`to` field has >1 recipient).

#### GoTo: [full test][send_receive_didkey_test]

```rust
// Creating message with multiple recipients.
let m = Message::new()
    .from("did:key:z6MkiTBz1ymuepAQ4HEHYSF1H8quG5GLVVQR3djdX3mDooWp")
    .to(&[
        "did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG",
        "did:key:z6MknGc3ocHs3zdPiJbnaaqDi58NGb4pk1Sp9WxWufuXSdxf",
    ])
    .as_jwe(&CryptoAlgorithm::XC20P, None);

let jwe = m.seal(&alice_private, None);
// Packing was ok?
assert!(jwe.is_ok());

let jwe = jwe.unwrap();

// Each of the recipients receive it in same way as before (direct with single recipient)
let received_first = Message::receive(&jwe, Some(&bobs_private), None, None);
let received_second = Message::receive(&jwe, Some(&carol_private), None, None);

// All good without any extra inputs
assert!(received_first.is_ok());
assert!(received_second.is_ok());
```

## 7. Working with `attachments`

### 7.1 Adding `Attachment`

```rust
use didcomm_rs::{Message, AttachmentBuilder, AttachmentDataBuilder};

let payload = b"some usefull data";
let mut m = Message:new();
    m.append_attachment(
        AttachmentBuilder::new(true)
            .with_id("best attachment")
            .with_data(
                AttachmentDataBuilder::new()
                    .with_raw_payload(payload)
            )
        );
```

or

```rust
use didcomm_rs::{Message, AttachmentBuilder, AttachmentDataBuilder};

let attachments: Vec<AttachmentBuilder>; // instantiate properly

let mut m = Message:new();

for attachment in attachments {
    m.append_attachment(attachment);
}
```

### 7.2 Parsing `Attachment`'s

```rust
// `m` is `receive()`'d instance of a `Message`

let something_im_looking_for = m.get_attachments().filter(|single| single.id == "id I'm looking for");
assert!(something_im_looking_for.next().is_some());

for found in something_im_looking_for {
    // process attachments
}

```

## 8. Threading

By default all new messages are created with random UUID as `thid` header value and with empty `pthid` value.

To reply to a message in thread with both `thid` and `pthid` copied  use `reply_to` method:

```rust

let m = Message::new()
    .reply_to(&received)
    // - other methods to form a message
    ;
```

To set parent thread id (or `pthid` header), use `with_parent` method:

```rust

let m = Message::new()
    .with_parent(&receievd)
    // - other methods to form a message
    ;
```

## 9. Other application-level headers and decorators

In order to satisfy any other header values universal method is present: `Message::add_header_field'
This method is backed up by a `HashMap` of <String, String>. If the key was present - it's value will be updated.

```rust

let m = Message::new()
    .add_header_field("key", "value")
    .add_header_field("~decorator", "value")
    // - other methods to form a message
    ;
```

To find if specific application level header is present and get it's value `get_application_params` method should be used.

```rust

let m: Message; // proprely instantiated received message

if let Some((my_key, my_value)) = m.get_application_params().filter(|(key, _)| key == "my_key").first();
```

# Plugable cryptography

In order to use your own implementation(s) of message crypto and/or signature algorithms implement these trait(s):

[`didcomm_rs::crypto::Cypher`][crypter]

[`didcomm_rs::crypto::Signer`][signer]

Don't use `default` feature - might change in future.

When implemented - use them instead of `CryptoAlgorithm` and `SignatureAlgorithm` from examples above.

## Strongly typed Message payload (body)

### GoTo: [full test][shape_desired_test]

In most cases application implementation would prefer to have strongly typed body of the message instead of raw `Vec<u8>`.
For this scenario `Shape` trait should be implemented for target type.

* First, let's define our target type. JSON in this example.

```rust
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct DesiredShape {
    num_field: usize,
    string_field: String,
}
```

* Next, implement `Shape` trait for it

```rust
impl Shape for DesiredShape {
    type Err = Error;
    fn shape(m: &Message) -> Result<DesiredShape, Error> {
        serde_json::from_str(&m.get_body().unwrap())
            .map_err(|e| Error::SerdeError(e))
    }
}
```

* Now we can call `shape()` on our `Message` and shape in in.
* In this example we expect JSON payload and use it's Deserializer to get our data, but your implementation can work with any serialization.

```rust
let body = r#"{"num_field":123,"string_field":"foobar"}"#.to_string();
let message = Message::new() // creating message
    .from("did:xyz:ulapcuhsatnpuhza930hpu34n_") // setting from
    .to(&["did::xyz:34r3cu403hnth03r49g03"]) // setting to
    .body(&body).unwrap(); // packing in some payload
let received_typed_body = DesiredShape::shape(&message).unwrap(); // Where m = Message
```

## Unimplemented changes from spec v2.1

DIDComm v2.1 [spec](https://identity.foundation/didcomm-messaging/spec/v2.1) has some changes which are unimplemented in the present `didcomm-rs` rust implementation.

 - `body` element is optional. It can be left empty if absent, [link](https://identity.foundation/didcomm-messaging/spec/v2.1/#overview:~:text=body%20%2D-,OPTIONAL,-.%20The%20body%20attribute)
 - `serviceEndpoint` has a minor variation in format, [link](https://identity.foundation/didcomm-messaging/spec/v2.1/#service-endpoint)

## Disclaimer

This is a sample implementation of the DIDComm V2 spec. The DIDComm V2 spec is still actively being developed by the DIDComm WG in the DIF and therefore subject to change.

<!-- Collect links to line numbers here to be able to maintain them easier -->
[crypter]: https://github.com/evannetwork/didcomm-rs/blob/master/src/crypto/mod.rs#L30
[send_receive_raw]: https://github.com/evannetwork/didcomm-rs/blob/master/tests/send_receive.rs#L16
[send_receive_encrypted_xc20p_json_test]: https://github.com/evannetwork/didcomm-rs/blob/master/tests/send_receive.rs#L42
[send_receive_mediated_encrypted_xc20p_json_test]: https://github.com/evannetwork/didcomm-rs/blob/master/tests/send_receive.rs#L84
[send_receive_direct_signed_and_encrypted_xc20p_test]: https://github.com/evannetwork/didcomm-rs/blob/master/tests/send_receive.rs#L164
[send_receive_didkey_test]: https://github.com/evannetwork/didcomm-rs/blob/master/src/messages/message.rs#L482
[shape_desired_test]: https://github.com/evannetwork/didcomm-rs/blob/main/tests/shape.rs#L21
[signer]: https://github.com/evannetwork/didcomm-rs/blob/master/src/crypto/mod.rs#L39
