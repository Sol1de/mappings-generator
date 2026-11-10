//! Minimal class file parser (JVMS chapter 4) and jar inventory. Reads names, descriptors,
//! access flags, hierarchy and a structural fingerprint; ignores code and attributes.

use crate::model::JarClass;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt;
use std::io::{Read, Seek};

/// Highest supported major version (Java 27).
pub const MAX_MAJOR: u16 = 71;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassError(pub String);

impl fmt::Display for ClassError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ClassError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassFile {
    pub major: u16,
    pub name: String,
    pub info: JarClass,
    /// Hash of super class, interface count, sorted field/method descriptors and string
    /// constants: stable across renames of the class's own members (spec/namespaces.md).
    pub fingerprint: [u8; 32],
}

struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ClassError> {
        let end = self
            .pos
            .checked_add(n)
            .filter(|&e| e <= self.data.len())
            .ok_or_else(|| ClassError("truncated class file".into()))?;
        let out = &self.data[self.pos..end];
        self.pos = end;
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8, ClassError> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, ClassError> {
        let b = self.take(2)?;
        Ok(u16::from_be_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Result<u32, ClassError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn skip_attributes(&mut self) -> Result<(), ClassError> {
        for _ in 0..self.u16()? {
            self.u16()?;
            let len = self.u32()? as usize;
            self.take(len)?;
        }
        Ok(())
    }
}

#[derive(Clone)]
enum Constant {
    Utf8(String),
    Class(u16),
    String(u16),
    Other,
}

/// Decodes modified UTF-8 (JVMS 4.4.7), including surrogate pairs and `C0 80`.
fn modified_utf8(bytes: &[u8]) -> Result<String, ClassError> {
    let mut units: Vec<u16> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let bad = || ClassError("invalid modified UTF-8".into());
    while i < bytes.len() {
        let b = bytes[i];
        if b & 0x80 == 0 {
            units.push(b as u16);
            i += 1;
        } else if b & 0xE0 == 0xC0 {
            let c = *bytes.get(i + 1).ok_or_else(bad)?;
            units.push(((b as u16 & 0x1F) << 6) | (c as u16 & 0x3F));
            i += 2;
        } else if b & 0xF0 == 0xE0 {
            let (c, d) = (
                *bytes.get(i + 1).ok_or_else(bad)?,
                *bytes.get(i + 2).ok_or_else(bad)?,
            );
            units.push(((b as u16 & 0x0F) << 12) | ((c as u16 & 0x3F) << 6) | (d as u16 & 0x3F));
            i += 3;
        } else {
            return Err(bad());
        }
    }
    String::from_utf16(&units).map_err(|_| bad())
}

pub fn parse(data: &[u8]) -> Result<ClassFile, ClassError> {
    let mut r = Reader { data, pos: 0 };
    if r.u32()? != 0xCAFE_BABE {
        return Err(ClassError("not a class file".into()));
    }
    let _minor = r.u16()?;
    let major = r.u16()?;
    if !(45..=MAX_MAJOR).contains(&major) {
        return Err(ClassError(format!(
            "unsupported class file version {major}"
        )));
    }
    let count = r.u16()?;
    let mut pool = vec![Constant::Other; count as usize];
    let mut i = 1;
    while i < count as usize {
        let tag = r.u8()?;
        pool[i] = match tag {
            1 => {
                let len = r.u16()? as usize;
                Constant::Utf8(modified_utf8(r.take(len)?)?)
            }
            7 => Constant::Class(r.u16()?),
            8 => Constant::String(r.u16()?),
            16 | 19 | 20 => {
                r.u16()?;
                Constant::Other
            }
            15 => {
                r.take(3)?;
                Constant::Other
            }
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => {
                r.u32()?;
                Constant::Other
            }
            5 | 6 => {
                r.take(8)?;
                i += 1; // 8-byte constants take two slots
                Constant::Other
            }
            other => return Err(ClassError(format!("unknown constant pool tag {other}"))),
        };
        i += 1;
    }
    let utf8 = |idx: u16| match pool.get(idx as usize) {
        Some(Constant::Utf8(s)) => Ok(s.clone()),
        _ => Err(ClassError(format!("constant {idx} is not UTF-8"))),
    };
    let class_name = |idx: u16| match pool.get(idx as usize) {
        Some(Constant::Class(n)) => utf8(*n),
        _ => Err(ClassError(format!("constant {idx} is not a class"))),
    };
    let access = r.u16()?;
    let name = class_name(r.u16()?)?;
    let super_name = match r.u16()? {
        0 => None,
        idx => Some(class_name(idx)?),
    };
    let interfaces = (0..r.u16()?)
        .map(|_| class_name(r.u16()?))
        .collect::<Result<Vec<_>, _>>()?;
    let members = |r: &mut Reader| -> Result<Vec<(String, String, u16)>, ClassError> {
        let n = r.u16()?;
        let mut out = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let acc = r.u16()?;
            let (mname, desc) = (utf8(r.u16()?)?, utf8(r.u16()?)?);
            r.skip_attributes()?;
            out.push((mname, desc, acc));
        }
        Ok(out)
    };
    let fields = members(&mut r)?;
    let methods = members(&mut r)?;
    r.skip_attributes()?;
    let mut hash = Sha256::new();
    hash.update(super_name.as_deref().unwrap_or("").as_bytes());
    hash.update((interfaces.len() as u32).to_be_bytes());
    for list in [&fields, &methods] {
        let mut descs: Vec<&str> = list.iter().map(|m| m.1.as_str()).collect();
        descs.sort_unstable();
        for d in descs {
            hash.update(d.as_bytes());
            hash.update([0]);
        }
        hash.update([1]);
    }
    let mut strings: Vec<&str> = pool
        .iter()
        .filter_map(|c| match c {
            Constant::String(i) => match pool.get(*i as usize) {
                Some(Constant::Utf8(s)) => Some(s.as_str()),
                _ => None,
            },
            _ => None,
        })
        .collect();
    strings.sort_unstable();
    for s in strings {
        hash.update(s.as_bytes());
        hash.update([0]);
    }
    let mut fingerprint = [0u8; 32];
    fingerprint.copy_from_slice(&hash.finalize());
    Ok(ClassFile {
        major,
        name,
        info: JarClass {
            access,
            super_name,
            interfaces,
            fields,
            methods,
        },
        fingerprint,
    })
}

/// Classes of a jar, keyed by internal name (taken from each class file, so Lunar caches whose
/// entries are named `a.b.C` work too). Non-class entries, `META-INF/` and `module-info` are
/// skipped; any other unreadable class is an error.
#[derive(Debug, Default)]
pub struct Inventory {
    pub classes: BTreeMap<String, JarClass>,
    pub fingerprints: BTreeMap<String, [u8; 32]>,
}

pub fn inventory(reader: impl Read + Seek) -> Result<Inventory, ClassError> {
    let mut zip =
        zip::ZipArchive::new(reader).map_err(|e| ClassError(format!("invalid jar: {e}")))?;
    let mut raw = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| ClassError(format!("invalid jar entry: {e}")))?;
        let name = entry
            .name()
            .map_err(|e| ClassError(format!("invalid jar entry name: {e}")))?
            .into_owned();
        if entry.is_dir() || name.starts_with("META-INF/") || name.ends_with("module-info.class") {
            continue;
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| ClassError(format!("{name}: {e}")))?;
        if bytes.starts_with(&[0xCA, 0xFE, 0xBA, 0xBE]) {
            raw.push((name, bytes));
        }
    }
    use rayon::prelude::*;
    let parsed: Vec<_> = raw
        .par_iter()
        .map(|(n, b)| parse(b).map_err(|e| ClassError(format!("{n}: {}", e.0))))
        .collect::<Result<_, _>>()?;
    let mut inv = Inventory::default();
    for c in parsed {
        inv.fingerprints.insert(c.name.clone(), c.fingerprint);
        if inv.classes.insert(c.name.clone(), c.info).is_some() {
            return Err(ClassError(format!("duplicate class {}", c.name)));
        }
    }
    Ok(inv)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Builds a minimal class file: `name extends sup` with the given fields and methods.
    pub fn class_bytes(
        name: &str,
        sup: &str,
        fields: &[(&str, &str, u16)],
        methods: &[(&str, &str, u16)],
        strings: &[&str],
    ) -> Vec<u8> {
        let mut pool: Vec<Vec<u8>> = Vec::new();
        let utf8 = |pool: &mut Vec<Vec<u8>>, s: &str| {
            let mut e = vec![1];
            e.extend((s.len() as u16).to_be_bytes());
            e.extend(s.as_bytes());
            pool.push(e);
            pool.len() as u16
        };
        let name_i = utf8(&mut pool, name);
        pool.push([vec![7], name_i.to_be_bytes().to_vec()].concat());
        let this = pool.len() as u16;
        let sup_i = utf8(&mut pool, sup);
        pool.push([vec![7], sup_i.to_be_bytes().to_vec()].concat());
        let sup = pool.len() as u16;
        pool.push(vec![5, 0, 0, 0, 0, 0, 0, 0, 1]); // long: takes two slots
        pool.push(Vec::new());
        for s in strings {
            let i = utf8(&mut pool, s);
            pool.push([vec![8], i.to_be_bytes().to_vec()].concat());
        }
        let member_bytes = |pool: &mut Vec<Vec<u8>>, list: &[(&str, &str, u16)]| {
            let mut out = (list.len() as u16).to_be_bytes().to_vec();
            for (n, d, a) in list {
                let (ni, di) = (utf8(pool, n), utf8(pool, d));
                out.extend(a.to_be_bytes());
                out.extend(ni.to_be_bytes());
                out.extend(di.to_be_bytes());
                out.extend(0u16.to_be_bytes());
            }
            out
        };
        let f = member_bytes(&mut pool, fields);
        let m = member_bytes(&mut pool, methods);
        let mut out = vec![0xCA, 0xFE, 0xBA, 0xBE, 0, 0, 0, 65];
        out.extend(((pool.len() + 1) as u16).to_be_bytes());
        for e in &pool {
            out.extend(e);
        }
        out.extend(0x21u16.to_be_bytes());
        out.extend(this.to_be_bytes());
        out.extend(sup.to_be_bytes());
        out.extend(0u16.to_be_bytes());
        out.extend(f);
        out.extend(m);
        out.extend(0u16.to_be_bytes());
        out
    }

    #[test]
    fn parses_generated_class() {
        let bytes = class_bytes(
            "a",
            "java/lang/Object",
            &[("b", "[[I", 9)],
            &[("<init>", "()V", 1), ("c", "(La;)V", 2)],
            &["hello"],
        );
        let c = parse(&bytes).unwrap();
        assert_eq!(c.name, "a");
        assert_eq!(c.info.super_name.as_deref(), Some("java/lang/Object"));
        assert_eq!(c.info.fields, [("b".to_string(), "[[I".to_string(), 9)]);
        assert_eq!(c.info.methods.len(), 2);
        let renamed = class_bytes(
            "z",
            "java/lang/Object",
            &[("q", "[[I", 9)],
            &[("<init>", "()V", 1), ("r", "(La;)V", 2)],
            &["hello"],
        );
        assert_eq!(
            parse(&renamed).unwrap().fingerprint,
            c.fingerprint,
            "fingerprint ignores member names"
        );
        let other = class_bytes("a", "java/lang/Object", &[], &[], &["bye"]);
        assert_ne!(parse(&other).unwrap().fingerprint, c.fingerprint);
    }

    #[test]
    fn rejects_bad_input_without_panic() {
        let bytes = class_bytes("a", "java/lang/Object", &[], &[], &[]);
        for cut in 0..bytes.len() - 1 {
            assert!(parse(&bytes[..cut]).is_err());
        }
        let mut future = bytes.clone();
        future[7] = 99;
        assert!(parse(&future).unwrap_err().0.contains("version 99"));
        assert!(parse(b"nope").is_err());
        assert_eq!(modified_utf8(&[0xC0, 0x80]).unwrap(), "\0");
        assert_eq!(
            modified_utf8(&[0xED, 0xA0, 0xBD, 0xED, 0xB8, 0x80]).unwrap(),
            "\u{1F600}"
        );
    }

    #[test]
    fn inventories_jars_including_dotted_entries() {
        use std::io::{Cursor, Write};
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("a.class", opts).unwrap();
            zip.write_all(&class_bytes("a", "java/lang/Object", &[], &[], &[]))
                .unwrap();
            zip.start_file("net.minecraft.class_1", opts).unwrap();
            zip.write_all(&class_bytes("net/minecraft/class_1", "a", &[], &[], &[]))
                .unwrap();
            zip.start_file("META-INF/MANIFEST.MF", opts).unwrap();
            zip.write_all(b"x").unwrap();
            zip.finish().unwrap();
        }
        buf.set_position(0);
        let inv = inventory(buf).unwrap();
        assert_eq!(
            inv.classes.keys().collect::<Vec<_>>(),
            ["a", "net/minecraft/class_1"]
        );
    }
}
