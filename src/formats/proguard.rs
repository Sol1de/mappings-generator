//! Mojang ProGuard mappings (`client_mappings`): `named -> obfuscated`.

use super::{ParseError, err};
use crate::model::{Kind, MappingSet, java_type_to_descriptor, remap_descriptor};
use std::collections::HashMap;

const F: &str = "proguard";

struct Entry {
    line: usize,
    kind: Kind,
    owner_obf: String,
    named: String,
    named_desc: String,
    obf: String,
}

fn internal(name: &str) -> String {
    name.replace('.', "/")
}

/// Reads ProGuard mappings into a set keyed by `official`, with a `mojang` namespace.
/// Descriptors are converted from Java syntax and remapped to `official`; line numbers are ignored.
pub fn read(text: &str) -> Result<MappingSet, ParseError> {
    let mut set = MappingSet::new(&["official", "mojang"]);
    let mut classes: HashMap<String, String> = HashMap::new();
    let mut entries = Vec::new();
    let mut current: Option<String> = None;
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let Some((left, obf)) = raw.trim().rsplit_once(" -> ") else {
            return err(F, line, "missing ' -> '");
        };
        if !raw.starts_with(char::is_whitespace) {
            let Some(obf) = obf.strip_suffix(':') else {
                return err(F, line, "class line must end with ':'");
            };
            let (named, obf) = (internal(left), internal(obf));
            if named.contains(' ') || obf.contains(' ') || obf.is_empty() {
                return err(F, line, "malformed class line");
            }
            if classes.insert(named.clone(), obf.clone()).is_some() {
                return err(F, line, format!("duplicate class {named}"));
            }
            set.set_class_name(&obf, 1, &named)
                .map_err(|e| ParseError {
                    format: F,
                    line,
                    message: e,
                })?;
            current = Some(obf);
            continue;
        }
        let Some(owner) = current.clone() else {
            return err(F, line, "member before any class");
        };
        let (kind, named, named_desc) = if let Some(open) = left.find('(') {
            // `[a:b:]ret name(args)[:c:d]`
            let Some(close) = left.rfind(')').filter(|&c| c > open) else {
                return err(F, line, "missing ')'");
            };
            let head = left[..open].trim_start_matches(|c: char| c.is_ascii_digit() || c == ':');
            let tail = &left[close + 1..];
            if !tail.is_empty() && !tail.split(':').skip(1).all(|n| n.parse::<u32>().is_ok()) {
                return err(F, line, "malformed line numbers");
            }
            let Some((ret, name)) = head.rsplit_once(' ') else {
                return err(F, line, "missing return type");
            };
            if name.contains('.') {
                // Inlined frame of another class's method: debug info, not a member.
                continue;
            }
            let mut desc = String::from("(");
            for arg in left[open + 1..close].split(',').filter(|a| !a.is_empty()) {
                match java_type_to_descriptor(arg.trim()) {
                    Some(d) if d != "V" => desc.push_str(&d),
                    _ => return err(F, line, format!("invalid parameter type {arg:?}")),
                }
            }
            desc.push(')');
            let Some(ret) = java_type_to_descriptor(ret) else {
                return err(F, line, format!("invalid type {ret:?}"));
            };
            desc.push_str(&ret);
            (Kind::Method, name, desc)
        } else {
            let Some((ty, name)) = left.rsplit_once(' ') else {
                return err(F, line, "malformed field");
            };
            match java_type_to_descriptor(ty) {
                Some(d) if d != "V" => (Kind::Field, name, d),
                _ => return err(F, line, format!("invalid field type {ty:?}")),
            }
        };
        if named.is_empty() || obf.is_empty() || obf.contains(' ') {
            return err(F, line, "malformed member");
        }
        entries.push(Entry {
            line,
            kind,
            owner_obf: owner,
            named: named.to_owned(),
            named_desc,
            obf: obf.to_owned(),
        });
    }
    for e in entries {
        let desc = remap_descriptor(&e.named_desc, |n| classes.get(n).map(String::as_str));
        set.set_member_name(&e.owner_obf, e.kind, (&e.obf, &desc), 1, &e.named)
            .map_err(|m| ParseError {
                format: F,
                line: e.line,
                message: m,
            })?;
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MemberKey;

    const SAMPLE: &str = "# comment
net.minecraft.client.Minecraft -> gfj:
    net.minecraft.client.Minecraft instance -> s
    int[][] grid -> t
    1:5:net.minecraft.client.Minecraft getInstance() -> Q
    6:9:void tick(int,java.lang.String[],net.minecraft.world.Entity):10:20 -> a
    6:9:void tick(int,java.lang.String[],net.minecraft.world.Entity):30:40 -> a
    7:7:void net.minecraft.Other.inlined():12:12 -> a
    10:11:void <init>() -> <init>
net.minecraft.world.Entity -> bsr:
";

    #[test]
    fn reads_and_converts_descriptors() {
        let s = read(SAMPLE).unwrap();
        let c = &s.classes["gfj"];
        assert_eq!(
            c.names[1].as_deref(),
            Some("net/minecraft/client/Minecraft")
        );
        assert_eq!(
            c.fields[&MemberKey::new("s", "Lgfj;")].names[1].as_deref(),
            Some("instance")
        );
        assert_eq!(
            c.fields[&MemberKey::new("t", "[[I")].names[1].as_deref(),
            Some("grid")
        );
        assert!(c.methods.contains_key(&MemberKey::new("Q", "()Lgfj;")));
        assert_eq!(
            c.methods[&MemberKey::new("a", "(I[Ljava/lang/String;Lbsr;)V")].names[1].as_deref(),
            Some("tick")
        );
        assert_eq!(c.methods.len(), 3);
    }

    #[test]
    fn rejects_malformed_with_line() {
        let e = read("a.B -> c:\n    int x\n").unwrap_err();
        assert_eq!(e.line, 2);
        let e = read("    int x -> a\n").unwrap_err();
        assert_eq!(e.line, 1);
        let e = read("a.B -> c:\n    void x -> a\n").unwrap_err();
        assert!(e.message.contains("field type"));
    }
}
