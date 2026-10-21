use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;

pub const PATH: &str = "/plugin";
pub const PORTS: [u16; 5] = [4433, 4434, 4435, 9201, 9202];
pub const MAX_MESSAGE: usize = 1024 * 1024;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    #[serde(rename = "functionID")]
    pub function_id: u32,
    pub args: Vec<Value>,
    #[serde(rename = "funcCallback")]
    pub callback: String,
    pub domain: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    Presence,
    Version,
    VendorAuthenticatedChallenge,
}

impl Request {
    pub fn parse(message: &[u8]) -> Result<Self> {
        if message.is_empty() || message.len() > MAX_MESSAGE {
            return Err("VNPT message exceeds the supported size".into());
        }
        let request: Self = serde_json::from_slice(message)?;
        validate_callback(&request.callback)?;
        if request.args.len() > 64
            || request.domain.len() > 2048
            || request.domain.chars().any(char::is_control)
        {
            return Err("invalid VNPT request bounds".into());
        }
        Ok(request)
    }

    pub fn probe(&self) -> Option<Probe> {
        match self.function_id {
            7 => Some(Probe::Presence),
            11 => Some(Probe::Version),
            13 => Some(Probe::VendorAuthenticatedChallenge),
            _ => None,
        }
    }
}

fn validate_callback(callback: &str) -> Result<()> {
    let mut bytes = callback.bytes();
    if callback.len() > 128
        || !bytes
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || matches!(first, b'_' | b'$'))
        || !bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'$'))
    {
        return Err("invalid VNPT callback name".into());
    }
    Ok(())
}

pub fn frame(payload: &str, callback: &str) -> Result<String> {
    validate_callback(callback)?;
    if payload.contains('*')
        || payload
            .len()
            .saturating_add(callback.len())
            .saturating_add(1)
            > MAX_MESSAGE
    {
        return Err("VNPT response cannot be represented in callback framing".into());
    }
    Ok(format!("{payload}*{callback}"))
}

pub fn unframe(message: &str, expected_callback: &str) -> Result<String> {
    validate_callback(expected_callback)?;
    if message.len() > MAX_MESSAGE {
        return Err("VNPT response exceeds the supported size".into());
    }
    let (payload, callback) = message.split_once('*').ok_or("missing VNPT callback")?;
    if callback != expected_callback {
        return Err("VNPT response callback does not match the request".into());
    }
    Ok(payload.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_vendor_client_request_shape() {
        let request = Request::parse(
            br#"{"functionID":11,"args":[""],"funcCallback":"macsignerVersionProbe","domain":"localhost"}"#,
        )
        .unwrap();
        assert_eq!(request.probe(), Some(Probe::Version));
        assert_eq!(request.args, vec![Value::String(String::new())]);
        assert_eq!(request.domain, "localhost");
    }

    #[test]
    fn matches_read_only_response_observed_from_installed_plugin() {
        let observed = "1.0.5.3*macsignerVersionProbe";
        assert_eq!(
            unframe(observed, "macsignerVersionProbe").unwrap(),
            "1.0.5.3"
        );
        assert_eq!(frame("1.0.5.3", "macsignerVersionProbe").unwrap(), observed);
        assert!(unframe(observed, "anotherCallback").is_err());
    }

    #[test]
    fn rejects_callback_injection_and_ambiguous_frames() {
        for callback in [
            "",
            "0callback",
            "callback*other",
            "callback();",
            "window.callback",
            "a\n",
        ] {
            assert!(frame("data", callback).is_err());
        }
        assert!(frame("first*second", "callbackDefault").is_err());
        assert!(unframe("first*callbackDefault*second", "callbackDefault").is_err());
    }

    #[test]
    fn rejects_missing_duplicate_and_unknown_request_fields() {
        for message in [
            br#"{"functionID":11,"args":[],"funcCallback":"cb"}"#.as_slice(),
            br#"{"functionID":11,"functionID":7,"args":[],"funcCallback":"cb","domain":"localhost"}"#.as_slice(),
            br#"{"functionID":11,"args":[],"funcCallback":"cb","domain":"localhost","extra":true}"#.as_slice(),
            br#"{"functionID":-1,"args":[],"funcCallback":"cb","domain":"localhost"}"#.as_slice(),
        ] {
            assert!(Request::parse(message).is_err());
        }
    }

    #[test]
    fn bounds_requests_and_responses() {
        assert!(Request::parse(&[]).is_err());
        assert!(Request::parse(&vec![b' '; MAX_MESSAGE + 1]).is_err());
        assert!(frame(&"x".repeat(MAX_MESSAGE), "cb").is_err());
        let request = serde_json::json!({
            "functionID": 11, "args": vec![Value::Null; 65],
            "funcCallback": "cb", "domain": "localhost"
        });
        assert!(Request::parse(&serde_json::to_vec(&request).unwrap()).is_err());
    }

    #[test]
    fn vendor_authentication_is_not_a_plain_presence_probe() {
        let mut request = Request::parse(
            br#"{"functionID":7,"args":[""],"funcCallback":"cb","domain":"localhost"}"#,
        )
        .unwrap();
        assert_eq!(request.probe(), Some(Probe::Presence));
        request.function_id = 13;
        assert_eq!(request.probe(), Some(Probe::VendorAuthenticatedChallenge));
        request.function_id = 9;
        assert_eq!(request.probe(), None);
    }
}
