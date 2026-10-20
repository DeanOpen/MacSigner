use pcsc::{Context, MAX_ATR_SIZE, Protocols, Scope, ShareMode};
use serde_json::{Value, json};
use std::error::Error;

/// Read-only PC/SC inventory. No APDU, PIN, or card mutation is performed.
pub fn readers() -> Result<Value, Box<dyn Error>> {
    let ctx = Context::establish(Scope::User)?;
    let mut result = Vec::new();
    for name in ctx.list_readers_owned()? {
        let reader = name.to_string_lossy().into_owned();
        match ctx.connect(&name, ShareMode::Shared, Protocols::ANY) {
            Ok(card) => {
                let (names_len, _) = card.status2_len()?;
                let mut names = vec![0_u8; names_len];
                let mut atr = [0_u8; MAX_ATR_SIZE];
                let status = card.status2(&mut names, &mut atr)?;
                result.push(json!({"reader": reader, "card_present": true,
                    "atr_hex": super::pkcs11::encode_hex(status.atr())}));
            }
            Err(err) => result.push(json!({"reader": reader, "card_present": false,
                "status": err.to_string()})),
        }
    }
    Ok(Value::Array(result))
}
