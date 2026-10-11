use serde_json::Value;

/// Canonical JSON encoding: recursively sort object keys, preserve array order,
/// and serialize without insignificant whitespace.
pub fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    fn normalize(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut entries: Vec<_> = map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                let mut out = serde_json::Map::new();
                for (k, v) in entries {
                    out.insert(k.clone(), normalize(v));
                }
                Value::Object(out)
            }
            Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
            other => other.clone(),
        }
    }

    serde_json::to_vec(&normalize(value)).expect("Value serialization cannot fail")
}

pub fn blake3_hex(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}
