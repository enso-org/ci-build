// FIXME FIXME FIXME
// Code copied from the primary repo json-rpc crate.
// Cannot be used normally because of all the nonsense with wasm-bindgen version.



use crate::prelude::*;

use shrinkwraprs::Shrinkwrap;


// ===============
// === Message ===
// ===============

/// All JSON-RPC messages bear `jsonrpc` version number.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Shrinkwrap)]
pub struct Message<T> {
    /// JSON-RPC Protocol version, should be 2.0.
    pub jsonrpc: Version,

    /// Payload, either a Request or Response or Notification in direct
    /// or serialized form.
    #[serde(flatten)]
    #[shrinkwrap(main_field)]
    pub payload: T,
}


// === Common Message Subtypes ===

/// A request message.
pub type RequestMessage<In> = Message<Request<MethodCall<In>>>;

/// A response message.
pub type ResponseMessage<Ret> = Message<Response<Ret>>;

/// A response message.
pub type NotificationMessage<Ret> = Message<Notification<MethodCall<Ret>>>;


// === `new` Functions ===

impl<T> Message<T> {
    /// Wraps given payload into a JSON-RPC 2.0 message.
    pub fn new(t: T) -> Message<T> {
        Message { jsonrpc: Version::V2, payload: t }
    }

    /// Construct a request message.
    pub fn new_request(id: Id, method: &str, params: T) -> RequestMessage<T> {
        let call = MethodCall { method: method.into(), params };
        let request = Request::new(id, call);
        Message::new(request)
    }

    /// Construct a successful response message.
    pub fn new_success(id: Id, result: T) -> ResponseMessage<T> {
        let result = Result::new_success(result);
        let response = Response { id, result };
        Message::new(response)
    }

    /// Construct a successful response message.
    pub fn new_error(
        id: Id,
        code: i64,
        message: String,
        data: Option<serde_json::Value>,
    ) -> ResponseMessage<T> {
        let result = Result::new_error(code, message, data);
        let response = Response { id, result };
        Message::new(response)
    }

    /// Construct a request message.
    pub fn new_notification(method: &'static str, params: T) -> NotificationMessage<T> {
        let call = MethodCall { method: method.into(), params };
        let notification = Notification(call);
        Message::new(notification)
    }
}


// ========================
// === Message Subparts ===
// ========================

/// An id identifying the call request.
///
/// Each request made by client should get a unique id (unique in a context of
/// the current session). Auto-incrementing integer is a common choice.
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Shrinkwrap,
)]
pub struct Id(pub i64);

/// JSON-RPC protocol version. Only 2.0 is supported.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Version {
    /// JSON-RPC 2.0 specification. The supported version.
    #[serde(rename = "2.0")]
    V2,
}

/// A non-notification request.
///
/// `Call` must be a type, that upon JSON serialization provides `method` and
/// `params` fields, like `MethodCall`.
#[derive(Serialize, Deserialize, Debug, PartialEq, Shrinkwrap)]
pub struct Request<Call> {
    /// An identifier for this request that will allow matching the response.
    pub id:   Id,
    #[serde(flatten)]
    #[shrinkwrap(main_field)]
    /// method and its params
    pub call: Call,
}

impl<M> Request<M> {
    /// Create a new request.
    pub fn new(id: Id, call: M) -> Request<M> {
        Request { id, call }
    }
}

/// A notification request.
///
/// `Call` must be a type, that upon JSON serialization provides `method` and
/// `params` fields, like `MethodCall`.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Notification<Call>(pub Call);

/// A response to a `Request`. Depending on `result` value it might be
/// successful or not.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Response<Res> {
    /// Identifier, matching the value given in `Request` when call was made.
    pub id:     Id,
    /// Call result.
    #[serde(flatten)]
    pub result: Result<Res>,
}

/// Result of the remote call — either a returned value or en error.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
#[allow(missing_docs)]
pub enum Result<Res> {
    /// Returned value of a successful call.
    Success(Success<Res>),
    /// Error value from a called that failed on the remote side.
    Error { error: Error },
}

impl<Res> Result<Res> {
    /// Construct a successful remote call result value.
    pub fn new_success(result: Res) -> Result<Res> {
        Result::Success(Success { result })
    }

    /// Construct a failed remote call result value.
    pub fn new_error(code: i64, message: String, data: Option<serde_json::Value>) -> Result<Res> {
        Result::Error { error: Error { code, message, data } }
    }

    /// Construct a failed remote call result value that bears no optional data.
    pub fn new_error_simple(code: i64, message: String) -> Result<Res> {
        Self::new_error(code, message, None)
    }
}

/// Value yield by a successful remote call.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Success<Ret> {
    /// A value returned from a successful remote call.
    pub result: Ret,
}

/// Error raised on a failed remote call.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct Error<Payload = serde_json::Value> {
    /// A number indicating what type of error occurred.
    pub code:    i64,
    /// A short description of the error.
    pub message: String,
    /// Optional value with additional information about the error.
    pub data:    Option<Payload>,
}

/// A message that can come from Server to Client — either a response or
/// notification.
#[derive(Serialize, Deserialize, Debug, PartialEq)]
#[serde(untagged)]
pub enum IncomingMessage {
    /// A response to a call made by client.
    Response(Response<serde_json::Value>),
    /// A notification call (initiated by the server).
    Notification(Notification<serde_json::Value>),
}

/// Partially decodes incoming message.
///
/// This checks if has `jsonrpc` version string, and whether it is a
/// response or a notification.
pub fn decode_incoming_message(message: &str) -> serde_json::Result<IncomingMessage> {
    use serde_json::from_str;
    use serde_json::from_value;
    use serde_json::Value;
    let message = from_str::<Message<Value>>(message)?;
    from_value::<IncomingMessage>(message.payload)
}

/// Message from server to client.
///
/// `In` is any serializable (or already serialized) representation of the
/// method arguments passed in this call.
#[derive(Serialize, Deserialize, Debug, PartialEq, Shrinkwrap)]
pub struct MethodCall<In> {
    /// Name of the method that is being called.
    pub method: String,
    /// Method arguments.
    #[shrinkwrap(main_field)]
    pub params: In,
}
