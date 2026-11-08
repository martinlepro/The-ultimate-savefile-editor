//! Minecraft Java (level.dat, etc.) : NBT big-endian, gzip.
use crate::doc::*;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use std::io::{Read, Write};

type B = Vec<u8>;

#[derive(Clone, Debug)]
pub enum Tag {
    Byte(i8),
    Short(i16),
    Int(i32),
    Long(i64),
    Float(f32),
    Double(f64),
    ByteArray(Vec<i8>),
    Str(B),
    List(u8, Vec<Tag>),
    Compound(Vec<(B, Tag)>),
    IntArray(Vec<i32>),
    LongArray(Vec<i64>),
}

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.p + n > self.b.len() {
            return Err("NBT tronqué".into());
        }
        let s = &self.b[self.p..self.p + n];
        self.p += n;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16, String> {
        Ok(i16::from_be_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(i32::from_be_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn i64(&mut self) -> Result<i64, String> {
        Ok(i64::from_be_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn s(&mut self) -> Result<B, String> {
        let n = self.i16()? as u16 as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn len(&mut self) -> Result<usize, String> {
        let n = self.i32()?;
        if n < 0 || n as usize > self.b.len() {
            return Err("longueur NBT invalide".into());
        }
        Ok(n as usize)
    }
    fn tag(&mut self, ty: u8, depth: u32) -> Result<Tag, String> {
        if depth > 512 {
            return Err("NBT trop profond".into());
        }
        Ok(match ty {
            1 => Tag::Byte(self.u8()? as i8),
            2 => Tag::Short(self.i16()?),
            3 => Tag::Int(self.i32()?),
            4 => Tag::Long(self.i64()?),
            5 => Tag::Float(f32::from_bits(self.i32()? as u32)),
            6 => Tag::Double(f64::from_bits(self.i64()? as u64)),
            7 => {
                let n = self.len()?;
                Tag::ByteArray(self.take(n)?.iter().map(|&b| b as i8).collect())
            }
            8 => Tag::Str(self.s()?),
            9 => {
                let t = self.u8()?;
                let n = self.len()?;
                let mut v = Vec::with_capacity(n.min(4096));
                for _ in 0..n {
                    v.push(self.tag(t, depth + 1)?);
                }
                Tag::List(t, v)
            }
            10 => {
                let mut v = vec![];
                loop {
                    let t = self.u8()?;
                    if t == 0 {
                        break;
                    }
                    let k = self.s()?;
                    v.push((k, self.tag(t, depth + 1)?));
                }
                Tag::Compound(v)
            }
            11 => {
                let n = self.len()?;
                Tag::IntArray((0..n).map(|_| self.i32()).collect::<Result<_, _>>()?)
            }
            12 => {
                let n = self.len()?;
                Tag::LongArray((0..n).map(|_| self.i64()).collect::<Result<_, _>>()?)
            }
            t => return Err(format!("type NBT inconnu : {t}")),
        })
    }
}

fn id(t: &Tag) -> u8 {
    match t {
        Tag::Byte(_) => 1,
        Tag::Short(_) => 2,
        Tag::Int(_) => 3,
        Tag::Long(_) => 4,
        Tag::Float(_) => 5,
        Tag::Double(_) => 6,
        Tag::ByteArray(_) => 7,
        Tag::Str(_) => 8,
        Tag::List(..) => 9,
        Tag::Compound(_) => 10,
        Tag::IntArray(_) => 11,
        Tag::LongArray(_) => 12,
    }
}

fn put_s(o: &mut B, s: &[u8]) {
    o.extend((s.len() as u16).to_be_bytes());
    o.extend(s);
}

fn write(o: &mut B, t: &Tag) {
    match t {
        Tag::Byte(v) => o.push(*v as u8),
        Tag::Short(v) => o.extend(v.to_be_bytes()),
        Tag::Int(v) => o.extend(v.to_be_bytes()),
        Tag::Long(v) => o.extend(v.to_be_bytes()),
        Tag::Float(v) => o.extend(v.to_bits().to_be_bytes()),
        Tag::Double(v) => o.extend(v.to_bits().to_be_bytes()),
        Tag::ByteArray(v) => {
            o.extend((v.len() as i32).to_be_bytes());
            o.extend(v.iter().map(|&b| b as u8));
        }
        Tag::Str(s) => put_s(o, s),
        Tag::List(ty, v) => {
            o.push(*ty);
            o.extend((v.len() as i32).to_be_bytes());
            v.iter().for_each(|x| write(o, x));
        }
        Tag::Compound(v) => {
            for (k, x) in v {
                o.push(id(x));
                put_s(o, k);
                write(o, x);
            }
            o.push(0);
        }
        Tag::IntArray(v) => {
            o.extend((v.len() as i32).to_be_bytes());
            v.iter().for_each(|x| o.extend(x.to_be_bytes()));
        }
        Tag::LongArray(v) => {
            o.extend((v.len() as i32).to_be_bytes());
            v.iter().for_each(|x| o.extend(x.to_be_bytes()));
        }
    }
}

pub struct NbtDoc {
    root_name: B,
    root: Tag,
    gz: bool,
}

impl NbtDoc {
    pub fn open(bytes: &[u8]) -> Result<Self, String> {
        let gz = bytes.starts_with(&[0x1f, 0x8b]);
        let raw = if gz {
            let mut o = vec![];
            GzDecoder::new(bytes).read_to_end(&mut o).map_err(|e| format!("gzip : {e}"))?;
            o
        } else {
            bytes.to_vec()
        };
        let mut r = R { b: &raw, p: 0 };
        if r.u8()? != 10 {
            return Err("pas un fichier NBT Java (racine non-compound). Bedrock n'est pas géré.".into());
        }
        let root_name = r.s()?;
        let root = r.tag(10, 0)?;
        Ok(NbtDoc { root_name, root, gz })
    }
}

fn child<'a>(cur: &'a Tag, st: &Step) -> Option<&'a Tag> {
    let nx = match cur {
        Tag::Compound(v) => st.alts.iter().find_map(|a| v.iter().find(|(k, _)| k == a.as_bytes()).map(|(_, t)| t))?,
        Tag::List(_, v) => v.get(st.alts[0].parse::<usize>().ok()?)?,
        _ => return None,
    };
    match (st.idx, nx) {
        (Idx::N(i), Tag::List(_, v)) => v.get(i),
        _ => Some(nx),
    }
}

fn child_mut<'a>(cur: &'a mut Tag, st: &Step) -> Option<&'a mut Tag> {
    let nx = match cur {
        Tag::Compound(v) => {
            let key = st.alts.iter().find(|a| v.iter().any(|(k, _)| k == a.as_bytes()))?;
            v.iter_mut().find(|(k, _)| k == key.as_bytes()).map(|(_, t)| t)?
        }
        Tag::List(_, v) => v.get_mut(st.alts[0].parse::<usize>().ok()?)?,
        _ => return None,
    };
    match (st.idx, nx) {
        (Idx::N(i), Tag::List(_, v)) => v.get_mut(i),
        (_, n) => Some(n),
    }
}

impl Doc for NbtDoc {
    fn get(&self, path: &str) -> Option<Scalar> {
        let mut cur = &self.root;
        for s in path.split('/') {
            cur = child(cur, &parse_step(s))?;
        }
        Some(match cur {
            Tag::Byte(v) => Scalar::Int(*v as i64),
            Tag::Short(v) => Scalar::Int(*v as i64),
            Tag::Int(v) => Scalar::Int(*v as i64),
            Tag::Long(v) => Scalar::Int(*v),
            Tag::Float(v) => Scalar::Float(*v as f64),
            Tag::Double(v) => Scalar::Float(*v),
            Tag::Str(s) => Scalar::Text(String::from_utf8_lossy(s).into_owned()),
            _ => return None,
        })
    }

    fn set(&mut self, path: &str, v: Scalar) -> Result<(), String> {
        let mut cur = &mut self.root;
        for s in path.split('/') {
            cur = child_mut(cur, &parse_step(s)).ok_or_else(|| format!("introuvable : {path}"))?;
        }
        let n = v.num();
        match cur {
            Tag::Byte(x) => *x = n.ok_or("nombre attendu")?.round().clamp(-128.0, 127.0) as i8,
            Tag::Short(x) => *x = n.ok_or("nombre attendu")?.round().clamp(i16::MIN as f64, i16::MAX as f64) as i16,
            Tag::Int(x) => *x = n.ok_or("nombre attendu")?.round().clamp(i32::MIN as f64, i32::MAX as f64) as i32,
            Tag::Long(x) => *x = match &v {
                Scalar::Int(i) => *i,
                _ => n.ok_or("nombre attendu")?.round() as i64,
            },
            Tag::Float(x) => *x = n.ok_or("nombre attendu")? as f32,
            Tag::Double(x) => *x = n.ok_or("nombre attendu")?,
            Tag::Str(x) => *x = v.text().into_bytes(),
            _ => return Err(format!("{path} n'est pas une valeur simple")),
        }
        Ok(())
    }

    fn expand(&self, path: &str) -> Vec<String> {
        let parts: Vec<&str> = path.split('/').collect();
        let Some(w) = first_wildcard(path) else { return vec![path.to_string()] };
        let mut cur = &self.root;
        for s in &parts[..w] {
            match child(cur, &parse_step(s)) {
                Some(c) => cur = c,
                None => return vec![],
            }
        }
        let keys: Vec<String> = match cur {
            Tag::List(_, v) => (0..v.len()).map(|i| i.to_string()).collect(),
            Tag::Compound(v) => v.iter().map(|(k, _)| String::from_utf8_lossy(k).into_owned()).collect(),
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
        let mut o = vec![10u8];
        put_s(&mut o, &self.root_name);
        write(&mut o, &self.root);
        if !self.gz {
            return Ok(o);
        }
        let mut g = GzEncoder::new(vec![], Compression::default());
        g.write_all(&o).map_err(|e| e.to_string())?;
        g.finish().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip() {
        let root = Tag::Compound(vec![(
            b"Data".to_vec(),
            Tag::Compound(vec![
                (b"LevelName".to_vec(), Tag::Str(b"World".to_vec())),
                (b"hardcore".to_vec(), Tag::Byte(0)),
                (b"Player".to_vec(), Tag::Compound(vec![
                    (b"Health".to_vec(), Tag::Float(20.0)),
                    (b"Pos".to_vec(), Tag::List(6, vec![Tag::Double(1.5), Tag::Double(64.0), Tag::Double(-3.0)])),
                    (b"Inventory".to_vec(), Tag::List(10, vec![Tag::Compound(vec![(b"id".to_vec(), Tag::Str(b"minecraft:dirt".to_vec())), (b"Count".to_vec(), Tag::Byte(5))])])),
                ])),
            ]),
        )]);
        let mut d = NbtDoc { root_name: vec![], root, gz: true };
        let enc = d.encode().unwrap();
        let mut d2 = NbtDoc::open(&enc).unwrap();
        assert_eq!(d2.get("Data/LevelName"), Some(Scalar::Text("World".into())));
        assert_eq!(d2.get("Data/Player/Pos/1"), Some(Scalar::Float(64.0)));
        d2.set("Data/Player/Health", Scalar::Float(3.5)).unwrap();
        d2.set("Data/Player/Inventory/0/Count?count", Scalar::Int(64)).unwrap();
        d2.set("Data/hardcore", Scalar::Bool(true)).unwrap();
        let d3 = NbtDoc::open(&d2.encode().unwrap()).unwrap();
        assert_eq!(d3.get("Data/Player/Health"), Some(Scalar::Float(3.5)));
        assert_eq!(d3.get("Data/Player/Inventory/0/Count"), Some(Scalar::Int(64)));
        assert_eq!(d3.get("Data/hardcore"), Some(Scalar::Int(1)));
        assert_eq!(d3.expand("Data/Player/Inventory/*"), vec!["Data/Player/Inventory/0"]);
        let _ = &mut d;
    }
}
