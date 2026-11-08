//! Éditeur XML « par patch » : le fichier est indexé une fois (spans d'octets) puis
//! chaque modification remplace uniquement le texte visé. Tout le reste du fichier
//! (BOM, indentation, ordre, attributs) reste identique octet pour octet.
use crate::doc::*;

const NIL: usize = usize::MAX;

#[derive(Clone, Copy)]
struct N {
    name_s: usize,
    name_e: usize,
    parent: usize,
    first: usize,
    last: usize,
    next: usize,
    tag_s: usize,
    open_e: usize,
    close_s: usize,
    end: usize,
    sc: bool,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Wrap {
    Plain,
    Gd,
}

pub struct XmlDoc {
    src: Vec<u8>,
    n: Vec<N>,
    wrap: Wrap,
}

fn find(src: &[u8], from: usize, pat: &[u8]) -> Result<usize, String> {
    if from > src.len() {
        return Err("XML tronqué".into());
    }
    src[from..]
        .windows(pat.len())
        .position(|w| w == pat)
        .map(|p| p + from)
        .ok_or_else(|| "XML tronqué".to_string())
}

fn index(src: &[u8]) -> Result<Vec<N>, String> {
    let len = src.len();
    let mut n = vec![N { name_s: 0, name_e: 0, parent: NIL, first: NIL, last: NIL, next: NIL, tag_s: 0, open_e: 0, close_s: len, end: len, sc: false }];
    let mut stack = vec![0usize];
    let mut i = 0;
    while i < len {
        match src[i..].iter().position(|&c| c == b'<') {
            Some(p) => i += p,
            None => break,
        }
        let rest = &src[i..];
        if rest.starts_with(b"<!--") {
            i = find(src, i + 4, b"-->")? + 3;
        } else if rest.starts_with(b"<![CDATA[") {
            i = find(src, i + 9, b"]]>")? + 3;
        } else if rest.starts_with(b"<?") {
            i = find(src, i + 2, b"?>")? + 2;
        } else if rest.starts_with(b"<!") {
            i = find(src, i + 2, b">")? + 1;
        } else if rest.starts_with(b"</") {
            let j = find(src, i + 2, b">")?;
            if stack.len() <= 1 {
                return Err("balise fermante orpheline".into());
            }
            let id = stack.pop().unwrap();
            n[id].close_s = i;
            n[id].end = j + 1;
            i = j + 1;
        } else {
            let name_s = i + 1;
            let mut k = name_s;
            while k < len && !matches!(src[k], b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>') {
                k += 1;
            }
            let name_e = k;
            while k < len && src[k] != b'>' {
                if src[k] == b'"' || src[k] == b'\'' {
                    let q = src[k];
                    k += 1;
                    while k < len && src[k] != q {
                        k += 1;
                    }
                }
                k += 1;
            }
            if k >= len {
                return Err("balise non fermée".into());
            }
            let sc = src[k - 1] == b'/';
            let id = n.len();
            let parent = *stack.last().unwrap();
            n.push(N { name_s, name_e, parent, first: NIL, last: NIL, next: NIL, tag_s: i, open_e: k + 1, close_s: if sc { k - 1 } else { k + 1 }, end: k + 1, sc });
            let last = n[parent].last;
            if last == NIL {
                n[parent].first = id;
            } else {
                n[last].next = id;
            }
            n[parent].last = id;
            if !sc {
                stack.push(id);
            }
            i = k + 1;
        }
    }
    if stack.len() != 1 {
        return Err("XML incomplet (balise non fermée)".into());
    }
    Ok(n)
}

fn unescape(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    let s = match s.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")) {
        Some(inner) => return inner.to_string(),
        None => s,
    };
    if !s.contains('&') {
        return s.into_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest: &str = &s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        if let Some(e) = rest.find(';') {
            let ent = &rest[1..e];
            let rep = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                _ => ent
                    .strip_prefix("#x")
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            if let Some(c) = rep {
                out.push(c);
                rest = &rest[e + 1..];
                continue;
            }
        }
        out.push('&');
        rest = &rest[1..];
    }
    out.push_str(rest);
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

impl XmlDoc {
    pub fn parse(src: Vec<u8>, wrap: Wrap) -> Result<Self, String> {
        let n = index(&src)?;
        if n[0].first == NIL {
            return Err("aucun élément XML".into());
        }
        Ok(XmlDoc { src, n, wrap })
    }

    fn name(&self, id: usize) -> &[u8] {
        &self.src[self.n[id].name_s..self.n[id].name_e]
    }

    fn text_of(&self, id: usize) -> Option<String> {
        let nd = &self.n[id];
        if nd.first != NIL {
            return None;
        }
        if nd.sc {
            return Some(String::new());
        }
        Some(unescape(&self.src[nd.open_e..nd.close_s]))
    }

    fn attr(&self, id: usize, want: &str) -> Option<String> {
        let nd = &self.n[id];
        let end = if nd.sc { nd.close_s } else { nd.open_e - 1 };
        let b = &self.src[nd.name_e..end.max(nd.name_e)];
        let mut i = 0;
        while i < b.len() {
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            let s = i;
            while i < b.len() && b[i] != b'=' && !b[i].is_ascii_whitespace() {
                i += 1;
            }
            let nm = &b[s..i];
            while i < b.len() && (b[i] == b'=' || b[i].is_ascii_whitespace()) {
                i += 1;
            }
            if i >= b.len() {
                break;
            }
            let q = b[i];
            i += 1;
            let vs = i;
            while i < b.len() && b[i] != q {
                i += 1;
            }
            if nm == want.as_bytes() {
                return Some(unescape(&b[vs..i.min(b.len())]));
            }
            i += 1;
        }
        None
    }

    fn child(&self, cur: usize, st: &Step) -> Option<usize> {
        let mut c = self.n[cur].first;
        if st.key {
            let want = st.alts[0];
            while c != NIL {
                if self.name(c) == b"k" && self.text_of(c).as_deref() == Some(want) {
                    let nx = self.n[c].next;
                    return if nx == NIL { None } else { Some(nx) };
                }
                c = self.n[c].next;
            }
            return None;
        }
        let want = match st.idx {
            Idx::N(i) => i,
            _ => 0,
        };
        let mut seen = 0;
        while c != NIL {
            let nm = self.name(c);
            if st.alts.iter().any(|a| *a == "*" || a.as_bytes() == nm) {
                if seen == want {
                    return Some(c);
                }
                seen += 1;
            }
            c = self.n[c].next;
        }
        None
    }

    fn resolve<'a>(&self, steps: impl Iterator<Item = &'a str>) -> Option<usize> {
        let mut cur = 0;
        for s in steps {
            cur = self.child(cur, &parse_step(s))?;
        }
        Some(cur)
    }

    fn set_text(&mut self, id: usize, s: &str) {
        let esc = escape(s);
        let nd = self.n[id];
        if nd.sc {
            let mut a = nd.close_s;
            while a > nd.name_e && self.src[a - 1].is_ascii_whitespace() {
                a -= 1;
            }
            let name = String::from_utf8_lossy(&self.src[nd.name_s..nd.name_e]).into_owned();
            let rep = format!(">{esc}</{name}>");
            let delta = rep.len() as isize - (nd.end - a) as isize;
            self.src.splice(a..nd.end, rep.bytes());
            let old_end = nd.end;
            for (i, m) in self.n.iter_mut().enumerate() {
                if i == id {
                    continue;
                }
                for f in [&mut m.name_s, &mut m.name_e, &mut m.tag_s, &mut m.open_e, &mut m.close_s, &mut m.end] {
                    if *f != NIL && *f >= old_end && !(i == 0) {
                        *f = (*f as isize + delta) as usize;
                    }
                }
            }
            self.n[0].end = self.src.len();
            self.n[0].close_s = self.src.len();
            let m = &mut self.n[id];
            m.sc = false;
            m.open_e = a + 1;
            m.close_s = a + 1 + esc.len();
            m.end = m.close_s + 3 + (m.name_e - m.name_s);
        } else {
            let old = nd.close_s - nd.open_e;
            let delta = esc.len() as isize - old as isize;
            let old_close = nd.close_s;
            self.src.splice(nd.open_e..nd.close_s, esc.bytes());
            for (i, m) in self.n.iter_mut().enumerate() {
                if i == id {
                    m.close_s = (m.close_s as isize + delta) as usize;
                    m.end = (m.end as isize + delta) as usize;
                    continue;
                }
                for f in [&mut m.name_s, &mut m.name_e, &mut m.tag_s, &mut m.open_e, &mut m.close_s, &mut m.end] {
                    if *f != NIL && *f >= old_close && i != 0 {
                        *f = (*f as isize + delta) as usize;
                    }
                }
            }
            self.n[0].end = self.src.len();
            self.n[0].close_s = self.src.len();
        }
    }
}

impl Doc for XmlDoc {
    fn get(&self, path: &str) -> Option<Scalar> {
        let parts: Vec<&str> = path.split('/').collect();
        if let Some(a) = parts.last().and_then(|l| l.strip_prefix('@')) {
            let id = self.resolve(parts[..parts.len() - 1].iter().copied())?;
            return self.attr(id, a).map(Scalar::Text);
        }
        let id = self.resolve(parts.iter().copied())?;
        self.text_of(id).map(Scalar::Text)
    }

    fn set(&mut self, path: &str, v: Scalar) -> Result<(), String> {
        let id = self.resolve(path.split('/')).ok_or_else(|| format!("introuvable : {path}"))?;
        let cur = self.text_of(id).ok_or_else(|| format!("{path} n'est pas une valeur simple"))?;
        let s = match v {
            Scalar::Bool(b) => match cur.trim() {
                "0" | "1" => (b as u8).to_string(),
                _ => b.to_string(),
            },
            other => other.text(),
        };
        if s != cur {
            self.set_text(id, &s);
        }
        Ok(())
    }

    fn expand(&self, path: &str) -> Vec<String> {
        let parts: Vec<&str> = path.split('/').collect();
        let Some(w) = first_wildcard(path) else { return vec![path.to_string()] };
        let Some(cur) = self.resolve(parts[..w].iter().copied()) else { return vec![] };
        let st = parse_step(parts[w]);
        let mut out = vec![];
        let mut counts: Vec<(Vec<u8>, usize)> = vec![];
        let mut c = self.n[cur].first;
        while c != NIL {
            let nm = self.name(c).to_vec();
            if st.alts.iter().any(|a| *a == "*" || a.as_bytes() == nm.as_slice()) {
                let k = match counts.iter_mut().find(|(n, _)| *n == nm) {
                    Some(e) => {
                        e.1 += 1;
                        e.1 - 1
                    }
                    None => {
                        counts.push((nm.clone(), 1));
                        0
                    }
                };
                let mut p = parts.clone();
                let rep = format!("{}[{}]", String::from_utf8_lossy(&nm), k);
                p[w] = &rep;
                out.push(p.join("/"));
            }
            c = self.n[c].next;
        }
        out
    }

    fn encode(&self) -> Result<Vec<u8>, String> {
        match self.wrap {
            Wrap::Plain => Ok(self.src.clone()),
            Wrap::Gd => crate::gd::pack(&self.src),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const T: &str = "\u{feff}<?xml version=\"1.0\"?>\n<A x=\"1\">\n  <b>12</b>\n  <c />\n  <l><i>1</i><i>2 &amp; 3</i></l>\n  <!-- k --><d t=\"q\"/><e>z</e>\n</A>\n";
    #[test]
    fn read_set_roundtrip() {
        let mut d = XmlDoc::parse(T.as_bytes().to_vec(), Wrap::Plain).unwrap();
        assert_eq!(d.get("A/b").unwrap().text(), "12");
        assert_eq!(d.get("A/l/i[1]").unwrap().text(), "2 & 3");
        assert_eq!(d.get("A/@x").unwrap().text(), "1");
        assert_eq!(d.get("A/d/@t").unwrap().text(), "q");
        d.set("A/b", Scalar::Int(123456)).unwrap();
        d.set("A/c", Scalar::Text("hé<".into())).unwrap();
        d.set("A/d", Scalar::Int(7)).unwrap();
        d.set("A/l/i[1]", Scalar::Text("a&b".into())).unwrap();
        d.set("A/e", Scalar::Int(9)).unwrap();
        let out = String::from_utf8(d.encode().unwrap()).unwrap();
        assert_eq!(out, "\u{feff}<?xml version=\"1.0\"?>\n<A x=\"1\">\n  <b>123456</b>\n  <c>hé&lt;</c>\n  <l><i>1</i><i>a&amp;b</i></l>\n  <!-- k --><d t=\"q\">7</d><e>9</e>\n</A>\n");
        assert_eq!(d.get("A/e").unwrap().text(), "9");
        assert_eq!(d.get("A/c").unwrap().text(), "hé<");
        assert_eq!(d.expand("A/l/i[*]"), vec!["A/l/i[0]", "A/l/i[1]"]);
    }
    #[test]
    fn plist_keys() {
        let t = "<?xml version=\"1.0\"?><plist><dict><k>a</k><d><k>6</k><s>10</s><k>7</k><s>5</s></d></dict></plist>";
        let mut d = XmlDoc::parse(t.as_bytes().to_vec(), Wrap::Plain).unwrap();
        assert_eq!(d.get("plist/dict/~a/~7").unwrap().text(), "5");
        d.set("plist/dict/~a/~6", Scalar::Int(99999)).unwrap();
        assert!(String::from_utf8(d.encode().unwrap()).unwrap().contains("<k>6</k><s>99999</s><k>7</k>"));
    }
}

#[cfg(test)]
mod real {
    use super::*;
    #[test]
    fn real_stardew_save() {
        let Ok(p) = std::env::var("SDV_TEST") else { return };
        let bytes = std::fs::read(p).unwrap();
        let t = std::time::Instant::now();
        let mut d = XmlDoc::parse(bytes.clone(), Wrap::Plain).unwrap();
        println!("index: {:?} pour {} octets, {} noeuds", t.elapsed(), bytes.len(), d.n.len());
        assert_eq!(d.encode().unwrap(), bytes, "aller-retour sans modification");
        assert_eq!(d.get("SaveGame/player/money").unwrap().text(), "1285");
        assert_eq!(d.get("SaveGame/player/name").unwrap().text(), "Test");
        assert_eq!(d.get("SaveGame/player/experiencePoints/int[3]").unwrap().text(), "111");
        let t = std::time::Instant::now();
        d.set("SaveGame/player/money", Scalar::Int(999999)).unwrap();
        d.set("SaveGame/player/deepestMineLevel", Scalar::Int(120)).unwrap();
        d.set("SaveGame/currentSeason", Scalar::Text("summer".into())).unwrap();
        d.set("SaveGame/player/experiencePoints/int[3]", Scalar::Int(15000)).unwrap();
        println!("4 set: {:?}", t.elapsed());
        let out = d.encode().unwrap();
        // re-parse : les valeurs doivent être lues au bon endroit après décalage des offsets
        let d2 = XmlDoc::parse(out.clone(), Wrap::Plain).unwrap();
        for k in ["SaveGame/player/money", "SaveGame/player/deepestMineLevel", "SaveGame/currentSeason", "SaveGame/player/experiencePoints/int[3]", "SaveGame/gameVersion", "SaveGame/player/name", "SaveGame/dayOfMonth"] {
            assert_eq!(d.get(k), d2.get(k), "{k}");
            println!("{k} = {:?}", d.get(k).map(|s| s.text()));
        }
        // seules les 4 valeurs ont changé : même taille moins/plus les deltas
        // 1285->999999 (+2), 5->120 (+2), spring->summer (0), 111->15000 (+2)
        assert_eq!(out.len() as isize - bytes.len() as isize, 6);
        // seuls ces 4 passages ont changé : on remet les valeurs d'origine et on retombe sur le fichier initial
        let mut back = d;
        back.set("SaveGame/player/money", Scalar::Int(1285)).unwrap();
        back.set("SaveGame/player/deepestMineLevel", Scalar::Int(5)).unwrap();
        back.set("SaveGame/currentSeason", Scalar::Text("spring".into())).unwrap();
        back.set("SaveGame/player/experiencePoints/int[3]", Scalar::Int(111)).unwrap();
        assert_eq!(back.encode().unwrap(), bytes, "modifier puis annuler redonne le fichier d'origine");
        let d = back;
        println!("friends: {:?}", d.expand("SaveGame/player/friendshipData/item[*]").len());
        println!("items: {:?}", d.expand("SaveGame/player/items/Item[*]").len());
        println!("name0: {:?}", d.get("SaveGame/player/items/Item[0]/name").map(|s| s.text()));
    }
}
