//! Hollow Knight (userN.dat) : en-tête .NET + base64( AES-256-ECB( JSON ) ).
//! Un fichier JSON clair (version Switch) est aussi accepté.
use crate::doc::*;
use aes::cipher::{generic_array::GenericArray, BlockDecrypt, BlockEncrypt, KeyInit};
use aes::Aes256;
use base64::{alphabet, engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig}, Engine};
use serde_json::Value;

const KEY: &[u8; 32] = b"UKu52ePUBwetZ9wNX88o54dnfKRu0T1l";
const HEADER: [u8; 22] = [0, 1, 0, 0, 0, 255, 255, 255, 255, 1, 0, 0, 0, 0, 0, 0, 0, 6, 1, 0, 0, 0];

pub struct HkDoc {
    v: Value,
    encrypted: bool,
}

fn b64() -> GeneralPurpose {
    GeneralPurpose::new(&alphabet::STANDARD, GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent))
}

pub fn decrypt(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 24 {
        return Err("fichier trop court".into());
    }
    let body = &bytes[HEADER.len()..bytes.len() - 1];
    let mut n = 0;
    while n < 5 && n < body.len() {
        n += 1;
        if body[n - 1] & 0x80 == 0 {
            break;
        }
    }
    let text: Vec<u8> = body[n..].iter().copied().filter(|b| !b.is_ascii_whitespace()).collect();
    let mut data = b64().decode(&text).map_err(|e| format!("base64 invalide : {e}"))?;
    if data.is_empty() || data.len() % 16 != 0 {
        return Err("longueur AES invalide (fichier corrompu ?)".into());
    }
    let c = Aes256::new(GenericArray::from_slice(KEY));
    for ch in data.chunks_exact_mut(16) {
        c.decrypt_block(GenericArray::from_mut_slice(ch));
    }
    let pad = *data.last().unwrap() as usize;
    if pad == 0 || pad > 16 {
        return Err("remplissage AES invalide".into());
    }
    data.truncate(data.len() - pad);
    Ok(data)
}

pub fn encrypt(json: &[u8]) -> Vec<u8> {
    let pad = 16 - json.len() % 16;
    let mut data = json.to_vec();
    data.extend(std::iter::repeat(pad as u8).take(pad));
    let c = Aes256::new(GenericArray::from_slice(KEY));
    for ch in data.chunks_exact_mut(16) {
        c.encrypt_block(GenericArray::from_mut_slice(ch));
    }
    let text = b64().encode(&data).into_bytes();
    let mut out = HEADER.to_vec();
    let mut l = text.len().min(0x7FFF_FFFF);
    while l >= 0x80 {
        out.push((l & 0x7F) as u8 | 0x80);
        l >>= 7;
    }
    out.push(l as u8);
    out.extend(text);
    out.push(11);
    out
}

impl HkDoc {
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let plain = bytes.iter().find(|b| !b.is_ascii_whitespace() && **b != 0xEF && **b != 0xBB && **b != 0xBF) == Some(&b'{');
        let json = if plain { bytes.to_vec() } else { decrypt(bytes)? };
        let json = json.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&json).to_vec();
        let v: Value = serde_json::from_slice(&json).map_err(|e| format!("JSON invalide : {e}"))?;
        Ok(HkDoc { v, encrypted: !plain })
    }
}

fn walk<'a>(mut cur: &'a Value, steps: &[&str]) -> Option<&'a Value> {
    for s in steps {
        let st = parse_step(s);
        cur = step_get(cur, &st)?;
    }
    Some(cur)
}

fn step_get<'a>(cur: &'a Value, st: &Step) -> Option<&'a Value> {
    let nx = match cur {
        Value::Object(m) => st.alts.iter().find_map(|a| m.get(*a))?,
        Value::Array(a) => a.get(st.alts[0].parse::<usize>().ok()?)?,
        _ => return None,
    };
    match st.idx {
        Idx::N(i) => nx.as_array()?.get(i),
        _ => Some(nx),
    }
}

fn to_scalar(v: &Value) -> Option<Scalar> {
    match v {
        Value::Bool(b) => Some(Scalar::Bool(*b)),
        Value::Number(n) => Some(match n.as_i64() {
            Some(i) => Scalar::Int(i),
            None => Scalar::Float(n.as_f64()?),
        }),
        Value::String(s) => Some(Scalar::Text(s.clone())),
        _ => None,
    }
}

fn number(v: &Scalar, int: bool) -> Option<Value> {
    let f = v.num()?;
    if int {
        serde_json::from_str(&(f.round() as i64).to_string()).ok()
    } else {
        serde_json::from_str(&f.to_string()).ok()
    }
}

impl Doc for HkDoc {
    fn get(&self, path: &str) -> Option<Scalar> {
        let p: Vec<&str> = path.split('/').collect();
        to_scalar(walk(&self.v, &p)?)
    }

    fn set(&mut self, path: &str, v: Scalar) -> Result<(), String> {
        let p: Vec<&str> = path.split('/').collect();
        let (last, pre) = p.split_last().ok_or("chemin vide")?;
        let mut cur = &mut self.v;
        for s in pre {
            let st = parse_step(s);
            cur = match cur {
                Value::Object(m) => st.alts.iter().find(|a| m.contains_key(**a)).and_then(|a| m.get_mut(*a)),
                Value::Array(a) => st.alts[0].parse::<usize>().ok().and_then(|i| a.get_mut(i)),
                _ => None,
            }
            .ok_or_else(|| format!("introuvable : {path}"))?;
        }
        let st = parse_step(last);
        match cur {
            Value::Object(m) => {
                let key = st.alts.iter().find(|a| m.contains_key(**a)).copied().unwrap_or(st.alts[0]);
                let new = match m.get(key) {
                    Some(Value::Bool(_)) => Value::Bool(v.flag().ok_or("booléen attendu")?),
                    Some(Value::Number(n)) => number(&v, n.is_i64() || n.is_u64()).ok_or("nombre attendu")?,
                    Some(Value::String(_)) => Value::String(v.text()),
                    Some(_) => return Err(format!("{path} n'est pas une valeur simple")),
                    None => match &v {
                        Scalar::Bool(b) => Value::Bool(*b),
                        Scalar::Int(_) | Scalar::Float(_) => number(&v, matches!(v, Scalar::Int(_))).ok_or("nombre attendu")?,
                        Scalar::Text(t) => Value::String(t.clone()),
                    },
                };
                m.insert(key.to_string(), new);
                Ok(())
            }
            Value::Array(a) => {
                let i: usize = st.alts[0].parse().map_err(|_| "index attendu")?;
                let slot = a.get_mut(i).ok_or("index hors limites")?;
                *slot = match slot {
                    Value::Bool(_) => Value::Bool(v.flag().ok_or("booléen attendu")?),
                    Value::Number(n) => number(&v, n.is_i64() || n.is_u64()).ok_or("nombre attendu")?,
                    _ => Value::String(v.text()),
                };
                Ok(())
            }
            _ => Err("parent invalide".into()),
        }
    }

    fn expand(&self, path: &str) -> Vec<String> {
        let parts: Vec<&str> = path.split('/').collect();
        let Some(w) = first_wildcard(path) else { return vec![path.to_string()] };
        let Some(cur) = walk(&self.v, &parts[..w]) else { return vec![] };
        let keys: Vec<String> = match cur {
            Value::Array(a) => (0..a.len()).map(|i| i.to_string()).collect(),
            Value::Object(m) => m.keys().cloned().collect(),
            _ => vec![],
        };
        keys.into_iter()
            .map(|k| {
                let mut p = parts.clone();
                p[w] = &k;
                p.join("/")
            })
            .collect()
    }

    fn encode(&self) -> Result<Vec<u8>, String> {
        let json = serde_json::to_vec(&self.v).map_err(|e| e.to_string())?;
        Ok(if self.encrypted { encrypt(&json) } else { json })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_edit() {
        let json = br#"{"playerData":{"geo":150,"health":5,"hasDash":false,"playTime":12.5,"name":"x","l":[1,2]},"sceneData":{}}"#;
        let enc = encrypt(json);
        let mut d = HkDoc::open(&enc).unwrap();
        assert_eq!(d.get("playerData/geo"), Some(Scalar::Int(150)));
        d.set("playerData/geo", Scalar::Float(99999.0)).unwrap();
        d.set("playerData/hasDash", Scalar::Bool(true)).unwrap();
        d.set("playerData/playTime", Scalar::Float(36000.25)).unwrap();
        d.set("playerData/l/1", Scalar::Int(7)).unwrap();
        let d2 = HkDoc::open(&d.encode().unwrap()).unwrap();
        assert_eq!(d2.get("playerData/geo"), Some(Scalar::Int(99999)));
        assert_eq!(d2.get("playerData/hasDash"), Some(Scalar::Bool(true)));
        assert_eq!(d2.get("playerData/playTime").unwrap().num(), Some(36000.25));
        assert_eq!(d2.get("playerData/l/1"), Some(Scalar::Int(7)));
        assert_eq!(d2.expand("playerData/l/*"), vec!["playerData/l/0", "playerData/l/1"]);
    }
}

#[cfg(test)]
mod xcheck {
    use super::*;
    use crate::doc::Doc;
    /// Compatibilité croisée avec le JS du dépôt (hollow-master) : XCHECK_DIR=/dossier
    #[test]
    fn cross_js() {
        let Ok(dir) = std::env::var("XCHECK_DIR") else { return };
        let json = r#"{"playerData":{"geo":777,"name":"café","big":[1,2,3]},"sceneData":{"x":1.5}}"#.as_bytes();
        std::fs::write(format!("{dir}/hk_rust.dat"), encrypt(json)).unwrap();
        if let Ok(b) = std::fs::read(format!("{dir}/hk_js.dat")) {
            let d = HkDoc::open(&b).unwrap();
            assert_eq!(d.get("playerData/geo"), Some(Scalar::Int(4242)));
            println!("HK: le fichier produit par le JS est lu par Rust");
        }
        let x = b"<?xml version=\"1.0\"?><plist gjver=\"2.0\" version=\"1.0\"><dict><k>GS_value</k><d><k>6</k><s>1234</s></d></dict></plist>";
        std::fs::write(format!("{dir}/gd_rust.dat"), crate::gd::pack(x).unwrap()).unwrap();
        if let Ok(b) = std::fs::read(format!("{dir}/gd_js.dat")) {
            let d = crate::gd::open(&b).unwrap();
            assert_eq!(d.get("plist/dict/~GS_value/~6").unwrap().text(), "4242");
            println!("GD: le fichier produit par le JS est lu par Rust");
        }
    }
}
