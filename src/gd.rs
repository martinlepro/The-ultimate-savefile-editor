//! Geometry Dash (CCGameManager.dat) : XOR 11 -> base64 url-safe -> gzip -> plist XML.
use crate::xml::{Wrap, XmlDoc};
use base64::{engine::general_purpose::URL_SAFE, Engine};
use flate2::{read::{GzDecoder, ZlibDecoder}, write::GzEncoder, Compression};
use std::io::{Read, Write};

pub fn open(bytes: &[u8]) -> Result<XmlDoc, String> {
    let head: Vec<u8> = bytes.iter().skip_while(|b| b.is_ascii_whitespace() || **b == 0xEF || **b == 0xBB || **b == 0xBF).take(5).copied().collect();
    if head == b"<?xml" {
        return XmlDoc::parse(bytes.to_vec(), Wrap::Plain);
    }
    let mut x: Vec<u8> = bytes.iter().map(|b| b ^ 11).collect();
    while matches!(x.last(), Some(0) | Some(b'\n') | Some(b'\r') | Some(b' ')) {
        x.pop();
    }
    let extra = x.len() % 4;
    x.truncate(x.len() - extra);
    let s: String = x.iter().map(|&b| b as char).collect::<String>().replace('+', "-").replace('/', "_");
    let raw = URL_SAFE.decode(s.trim_end_matches('=')).or_else(|_| {
        base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.trim_end_matches('='))
    }).map_err(|e| format!("base64 invalide : {e}"))?;
    let mut out = Vec::new();
    if GzDecoder::new(&raw[..]).read_to_end(&mut out).is_err() {
        out.clear();
        ZlibDecoder::new(&raw[..]).read_to_end(&mut out).map_err(|e| format!("décompression impossible : {e}"))?;
    }
    XmlDoc::parse(out, Wrap::Gd)
}

pub fn pack(xml: &[u8]) -> Result<Vec<u8>, String> {
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(xml).map_err(|e| e.to_string())?;
    let z = gz.finish().map_err(|e| e.to_string())?;
    Ok(URL_SAFE.encode(z).bytes().map(|b| b ^ 11).collect())
}

#[cfg(test)]
mod tests {
    use crate::doc::*;
    #[test]
    fn roundtrip() {
        let xml = b"<?xml version=\"1.0\"?><plist gjver=\"2.0\" version=\"1.0\"><dict><k>GS_value</k><d><k>6</k><s>1234</s></d></dict></plist>";
        let enc = super::pack(xml).unwrap();
        let mut d = super::open(&enc).unwrap();
        assert_eq!(d.get("plist/dict/~GS_value/~6").unwrap().text(), "1234");
        d.set("plist/dict/~GS_value/~6", Scalar::Int(99)).unwrap();
        let d2 = super::open(&d.encode().unwrap()).unwrap();
        assert_eq!(d2.get("plist/dict/~GS_value/~6").unwrap().text(), "99");
    }
}
