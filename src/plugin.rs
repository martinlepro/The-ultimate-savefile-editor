//! Exécute `savecodecs.wasm` (le code du pack qui comprend les sauvegardes) dans un bac à sable WebAssembly (wasmi,
//! interpréteur 100 % Rust : même comportement sur Windows, Linux et Android). Le module n'a aucune importation :
//! il ne peut ni lire de fichier, ni accéder au réseau, ni voir autre chose que les octets qu'on lui passe.
use crate::doc::{Doc, Scalar};
use std::cell::RefCell;
use std::rc::Rc;
use wasmi::{Engine, Instance, Memory, Module, Store, TypedFunc};

pub const ABI: u32 = 1;

pub struct Plugin {
    store: Store<()>,
    mem: Memory,
    alloc: TypedFunc<i32, i32>,
    free: TypedFunc<(i32, i32), ()>,
    res_ptr: TypedFunc<(), i32>,
    res_len: TypedFunc<(), i32>,
    open: TypedFunc<(i32, i32, i32, i32), i32>,
    close: TypedFunc<i32, ()>,
    get: TypedFunc<(i32, i32, i32), i32>,
    set: TypedFunc<(i32, i32, i32, i32, i32, i32), i32>,
    expand: TypedFunc<(i32, i32, i32), i32>,
    encode: TypedFunc<i32, i32>,
    codecs: Vec<String>,
}

fn e<T: std::fmt::Display>(x: T) -> String {
    format!("codecs : {x}")
}

impl Plugin {
    pub fn load(wasm: &[u8]) -> Result<Plugin, String> {
        let engine = Engine::default();
        let module = Module::new(&engine, wasm).map_err(e)?;
        if module.imports().count() != 0 {
            return Err("codecs refusés : le module demande des accès au système".into());
        }
        let mut store = Store::new(&engine, ());
        let linker = wasmi::Linker::<()>::new(&engine);
        let inst: Instance = linker.instantiate_and_start(&mut store, &module).map_err(e)?;
        macro_rules! f {
            ($n:expr) => {
                inst.get_typed_func(&store, $n).map_err(|_| format!("codecs : fonction « {} » absente", $n))?
            };
        }
        let abi: TypedFunc<(), i32> = f!("ss_abi");
        let v = abi.call(&mut store, ()).map_err(e)? as u32;
        if v != ABI {
            return Err(format!("codecs : ABI {v} non gérée (cette version de SaveStudio comprend l'ABI {ABI})"));
        }
        let mem = inst.get_memory(&store, "memory").ok_or("codecs : mémoire absente")?;
        let list: TypedFunc<(), i32> = f!("ss_codecs");
        let mut p = Plugin {
            alloc: f!("ss_alloc"),
            free: f!("ss_free"),
            res_ptr: f!("ss_result_ptr"),
            res_len: f!("ss_result_len"),
            open: f!("ss_open"),
            close: f!("ss_close"),
            get: f!("ss_get"),
            set: f!("ss_set"),
            expand: f!("ss_expand"),
            encode: f!("ss_encode"),
            store,
            mem,
            codecs: vec![],
        };
        list.call(&mut p.store, ()).map_err(e)?;
        p.codecs = String::from_utf8_lossy(&p.result()?).split(',').map(|s| s.to_string()).collect();
        Ok(p)
    }

    pub fn codecs(&self) -> &[String] {
        &self.codecs
    }

    fn result(&mut self) -> Result<Vec<u8>, String> {
        let ptr = self.res_ptr.call(&mut self.store, ()).map_err(e)? as usize;
        let len = self.res_len.call(&mut self.store, ()).map_err(e)? as usize;
        let mut buf = vec![0u8; len];
        if len > 0 {
            self.mem.read(&self.store, ptr, &mut buf).map_err(e)?;
        }
        Ok(buf)
    }

    fn text_result(&mut self) -> String {
        self.result().map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()
    }

    /// Copie des octets dans la mémoire du module ; à libérer avec `release`.
    fn put(&mut self, b: &[u8]) -> Result<(i32, i32), String> {
        let len = b.len() as i32;
        let ptr = self.alloc.call(&mut self.store, len).map_err(e)?;
        if !b.is_empty() {
            self.mem.write(&mut self.store, ptr as usize, b).map_err(e)?;
        }
        Ok((ptr, len))
    }

    fn release(&mut self, p: (i32, i32)) {
        let _ = self.free.call(&mut self.store, p);
    }
}

pub struct WasmDoc {
    p: Rc<RefCell<Plugin>>,
    h: i32,
}

pub fn open(p: &Rc<RefCell<Plugin>>, codec: &str, bytes: &[u8]) -> Result<Box<dyn Doc>, String> {
    let mut g = p.borrow_mut();
    let g = &mut *g;
    if !g.codecs.iter().any(|c| c == codec) {
        return Err(format!("Ce pack de jeux ne sait pas lire le format « {codec} »"));
    }
    let (c, d) = (g.put(codec.as_bytes())?, g.put(bytes)?);
    let r = g.open.call(&mut g.store, (c.0, c.1, d.0, d.1));
    g.release(c);
    g.release(d);
    let h = r.map_err(e)?;
    if h == 0 {
        return Err(g.text_result());
    }
    Ok(Box::new(WasmDoc { p: p.clone(), h }))
}

impl Doc for WasmDoc {
    fn get(&self, path: &str) -> Option<Scalar> {
        let mut g = self.p.try_borrow_mut().ok()?;
        let g = &mut *g;
        let pp = g.put(path.as_bytes()).ok()?;
        let tag = g.get.call(&mut g.store, (self.h, pp.0, pp.1));
        g.release(pp);
        let tag = tag.ok()?;
        if tag == 0 {
            return None;
        }
        let t = g.text_result();
        Some(match tag {
            1 => Scalar::Bool(t == "true"),
            2 => t.parse().map(Scalar::Int).unwrap_or(Scalar::Text(t)),
            3 => t.parse().map(Scalar::Float).unwrap_or(Scalar::Text(t)),
            _ => Scalar::Text(t),
        })
    }

    fn set(&mut self, path: &str, v: Scalar) -> Result<(), String> {
        let mut g = self.p.try_borrow_mut().map_err(|_| "codecs occupés".to_string())?;
        let g = &mut *g;
        let (tag, s) = match &v {
            Scalar::Bool(b) => (1, b.to_string()),
            Scalar::Int(i) => (2, i.to_string()),
            Scalar::Float(f) => (3, f.to_string()),
            Scalar::Text(t) => (4, t.clone()),
        };
        let (pp, vv) = (g.put(path.as_bytes())?, g.put(s.as_bytes())?);
        let r = g.set.call(&mut g.store, (self.h, pp.0, pp.1, tag, vv.0, vv.1));
        g.release(pp);
        g.release(vv);
        match r.map_err(e)? {
            0 => Ok(()),
            _ => Err(g.text_result()),
        }
    }

    fn expand(&self, path: &str) -> Vec<String> {
        let Ok(mut g) = self.p.try_borrow_mut() else { return vec![] };
        let g = &mut *g;
        let Ok(pp) = g.put(path.as_bytes()) else { return vec![] };
        let n = g.expand.call(&mut g.store, (self.h, pp.0, pp.1));
        g.release(pp);
        match n {
            Ok(n) if n > 0 => g.text_result().split('\n').map(|s| s.to_string()).collect(),
            _ => vec![],
        }
    }

    fn encode(&self) -> Result<Vec<u8>, String> {
        let mut g = self.p.try_borrow_mut().map_err(|_| "codecs occupés".to_string())?;
        let g = &mut *g;
        match g.encode.call(&mut g.store, self.h).map_err(e)? {
            0 => g.result(),
            _ => Err(g.text_result()),
        }
    }
}

impl Drop for WasmDoc {
    fn drop(&mut self) {
        if let Ok(mut g) = self.p.try_borrow_mut() {
            let g = &mut *g;
            let _ = g.close.call(&mut g.store, self.h);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Essai contre le vrai module : `SAVESTUDIO_TEST_PACK=/chemin/depot-games cargo test`.
    #[test]
    fn real_wasm_roundtrip() {
        let Ok(dir) = std::env::var("SAVESTUDIO_TEST_PACK") else { return };
        let wasm = std::fs::read(std::path::Path::new(&dir).join("wasm/savecodecs.wasm")).unwrap();
        let p = Rc::new(RefCell::new(Plugin::load(&wasm).unwrap()));
        assert!(p.borrow().codecs().iter().any(|c| c == "xml"));
        let xml = br#"<?xml version="1.0" encoding="utf-8"?><A><b>12</b><c>hi</c><l><i>1</i><i>2</i></l></A>"#;
        let mut d = open(&p, "xml", xml).unwrap();
        assert_eq!(d.get("A/c"), Some(Scalar::Text("hi".into())));
        assert_eq!(d.get("A/zzz"), None);
        d.set("A/b", Scalar::Int(77)).unwrap();
        assert!(d.set("A/b", Scalar::Int(1)).is_ok());
        d.set("A/b", Scalar::Int(77)).unwrap();
        let out = String::from_utf8(d.encode().unwrap()).unwrap();
        assert!(out.contains("<b>77</b>"), "{out}");
        assert!(open(&p, "zzz", xml).is_err());
        assert!(open(&p, "xml", b"not xml at all <<<").is_err());
    }

    /// Mesure sur un gros fichier XML : `SAVESTUDIO_BENCH_XML=/chemin SAVESTUDIO_TEST_PACK=... cargo test --release -- --nocapture bench`
    #[test]
    fn bench_big_xml() {
        let (Ok(dir), Ok(f)) = (std::env::var("SAVESTUDIO_TEST_PACK"), std::env::var("SAVESTUDIO_BENCH_XML")) else { return };
        let wasm = std::fs::read(std::path::Path::new(&dir).join("wasm/savecodecs.wasm")).unwrap();
        let p = Rc::new(RefCell::new(Plugin::load(&wasm).unwrap()));
        let bytes = std::fs::read(f).unwrap();
        let t = std::time::Instant::now();
        let mut d = open(&p, "xml", &bytes).unwrap();
        println!("ouverture de {} Mo : {:?}", bytes.len() / 1_000_000, t.elapsed());
        let t = std::time::Instant::now();
        d.set("SaveGame/player/money", Scalar::Int(5)).unwrap();
        let out = d.encode().unwrap();
        println!("set + encode : {:?} ({} octets)", t.elapsed(), out.len());
    }

    #[test]
    fn modules_with_imports_are_refused() {
        // (module (import "env" "x" (func)))
        let wasm = [0, 0x61, 0x73, 0x6d, 1, 0, 0, 0, 2, 9, 1, 3, b'e', b'n', b'v', 1, b'x', 0, 0];
        assert!(Plugin::load(&wasm).is_err());
    }
}
