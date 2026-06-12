use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct Request {
    pub id: u64,
    pub cmd: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub data: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: u64, data: Value) -> Self {
        Self {
            id,
            ok: true,
            data,
            error: None,
        }
    }
    pub fn err(id: u64, msg: impl Into<String>) -> Self {
        Self {
            id,
            ok: false,
            data: Value::Null,
            error: Some(msg.into()),
        }
    }
}

/// Broadcast event — one JSON line: {"event": "...", "data": {...}}
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Event {
    pub event: String,
    pub data: Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_parses_with_and_without_params() {
        let r: Request = serde_json::from_str(r#"{"id":1,"cmd":"play"}"#).unwrap();
        assert_eq!(r.cmd, "play");
        assert!(r.params.is_null());
        let r: Request =
            serde_json::from_str(r#"{"id":2,"cmd":"rate","params":{"value":0.8}}"#).unwrap();
        assert_eq!(r.params["value"], 0.8);
    }

    #[test]
    fn responses_serialize_compactly() {
        let ok = serde_json::to_string(&Response::ok(1, serde_json::Value::Null)).unwrap();
        assert_eq!(ok, r#"{"id":1,"ok":true}"#);
        let err = serde_json::to_string(&Response::err(2, "nope")).unwrap();
        assert_eq!(err, r#"{"id":2,"ok":false,"error":"nope"}"#);
    }
}
