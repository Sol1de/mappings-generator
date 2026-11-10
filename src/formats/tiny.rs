//! Tiny v1 and v2 (Fabric Intermediary, Yarn, Lunar Mapping Project).

use super::{ParseError, err};
use crate::model::{Kind, MappingSet};

const F: &str = "tiny";

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Applies the names of one entry; empty names mean "no name" (the first one is required).
fn apply(
    set: &mut MappingSet,
    line: usize,
    target: Option<(Kind, &str, &str)>,
    names: &[String],
) -> Result<(), ParseError> {
    if names.len() != set.namespaces.len() {
        return err(
            F,
            line,
            format!(
                "expected {} names, found {}",
                set.namespaces.len(),
                names.len()
            ),
        );
    }
    let Some(first) = names.first().filter(|n| !n.is_empty()) else {
        return err(F, line, "missing name in the first namespace");
    };
    for (ns, name) in names
        .iter()
        .enumerate()
        .skip(1)
        .filter(|(_, n)| !n.is_empty())
    {
        let result = match target {
            None => set.set_class_name(first, ns, name),
            Some((kind, owner, desc)) => set.set_member_name(owner, kind, (first, desc), ns, name),
        };
        result.map_err(|message| ParseError {
            format: F,
            line,
            message,
        })?;
    }
    // Entries without other names still exist in the first namespace.
    match target {
        None => _ = set.class_mut(first),
        Some((kind, owner, desc)) => _ = set.member_mut(owner, kind, first, desc),
    }
    Ok(())
}

/// Reads Tiny v1 or v2; parameters, local variables, comments and properties are ignored.
pub fn read(text: &str) -> Result<MappingSet, ParseError> {
    let mut lines = text.lines().enumerate();
    let Some((_, header)) = lines.next() else {
        return err(F, 1, "empty file");
    };
    let cols: Vec<&str> = header.split('\t').collect();
    match cols.as_slice() {
        ["v1", ns @ ..] if !ns.is_empty() => read_v1(ns, lines),
        ["tiny", "2", _, ns @ ..] if !ns.is_empty() => read_v2(ns, lines),
        _ => err(F, 1, "unknown tiny header"),
    }
}

fn read_v1<'a>(
    ns: &[&str],
    lines: impl Iterator<Item = (usize, &'a str)>,
) -> Result<MappingSet, ParseError> {
    let mut set = MappingSet::new(ns);
    for (i, raw) in lines {
        let line = i + 1;
        if raw.is_empty() || raw.starts_with('#') {
            continue;
        }
        let cols: Vec<String> = raw.split('\t').map(str::to_owned).collect();
        match cols[0].as_str() {
            "CLASS" => apply(&mut set, line, None, &cols[1..])?,
            "FIELD" | "METHOD" if cols.len() >= 4 => {
                let kind = if cols[0] == "FIELD" {
                    Kind::Field
                } else {
                    Kind::Method
                };
                apply(&mut set, line, Some((kind, &cols[1], &cols[2])), &cols[3..])?;
            }
            other => return err(F, line, format!("unknown entry {other:?}")),
        }
    }
    Ok(set)
}

fn read_v2<'a>(
    ns: &[&str],
    lines: impl Iterator<Item = (usize, &'a str)>,
) -> Result<MappingSet, ParseError> {
    let mut set = MappingSet::new(ns);
    let mut escaped = false;
    let mut in_header = true;
    let mut class: Option<String> = None;
    for (i, raw) in lines {
        let line = i + 1;
        if raw.is_empty() {
            continue;
        }
        let depth = raw.len() - raw.trim_start_matches('\t').len();
        let cols: Vec<&str> = raw[depth..].split('\t').collect();
        if in_header && depth == 1 {
            escaped |= cols[0] == "escaped-names";
            continue;
        }
        in_header = false;
        let names = |from: usize| -> Vec<String> {
            cols[from..]
                .iter()
                .map(|n| {
                    if escaped {
                        unescape(n)
                    } else {
                        (*n).to_owned()
                    }
                })
                .collect()
        };
        match (depth, cols[0]) {
            (0, "c") => {
                let n = names(1);
                apply(&mut set, line, None, &n)?;
                class = Some(n[0].clone());
            }
            (1, "f" | "m") if cols.len() >= 3 => {
                let Some(owner) = class.clone() else {
                    return err(F, line, "member outside a class");
                };
                let kind = if cols[0] == "f" {
                    Kind::Field
                } else {
                    Kind::Method
                };
                apply(&mut set, line, Some((kind, &owner, cols[1])), &names(2))?;
            }
            (1, "c") | (2.., _) => {} // comments, parameters, local variables
            _ => {
                return err(
                    F,
                    line,
                    format!("unexpected entry {:?} at depth {depth}", cols[0]),
                );
            }
        }
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MemberKey;

    #[test]
    fn reads_v2_with_escapes_and_ignored_sections() {
        let text = "tiny\t2\t0\tofficial\tintermediary\tnamed\n\tescaped-names\n\
c\tgfj\tnet/minecraft/class_310\tnet/minecraft/client/MinecraftClient\n\
\tc\ta comment\n\
\tf\tLgfj;\ts\tfield_1700\tinstance\n\
\tm\t()Lgfj;\tQ\tmethod_1551\t\n\
\t\tp\t1\t\t\tparam\n\
\t\tc\tdoc\\twith tab\n\
c\ta\tnet/minecraft/class_1\tna\\\\me\n";
        let s = read(text).unwrap();
        assert_eq!(s.namespaces, ["official", "intermediary", "named"]);
        let c = &s.classes["gfj"];
        assert_eq!(
            c.fields[&MemberKey::new("s", "Lgfj;")].names[1].as_deref(),
            Some("field_1700")
        );
        let m = &c.methods[&MemberKey::new("Q", "()Lgfj;")];
        assert_eq!(
            (m.names[1].as_deref(), m.names[2].as_deref()),
            (Some("method_1551"), None)
        );
        assert_eq!(s.classes["a"].names[2].as_deref(), Some("na\\me"));
    }

    #[test]
    fn reads_v1() {
        let text = "v1\tofficial\tintermediary\n# INTERMEDIARY-COUNTER class 1\n\
CLASS\ta\tnet/minecraft/class_1\nFIELD\ta\tI\tb\tfield_2\nMETHOD\ta\t()V\tc\tmethod_3\n";
        let s = read(text).unwrap();
        assert!(
            s.classes["a"]
                .methods
                .contains_key(&MemberKey::new("c", "()V"))
        );
        assert_eq!(
            s.classes["a"].fields[&MemberKey::new("b", "I")].names[1].as_deref(),
            Some("field_2")
        );
    }

    #[test]
    fn rejects_malformed() {
        assert_eq!(read("tiny\t2\t0\ta\tb\nc\tx\n").unwrap_err().line, 2);
        assert_eq!(
            read("tiny\t2\t0\ta\tb\nc\tx\ty\n\tz\tI\tx\ty\n")
                .unwrap_err()
                .line,
            3
        );
        assert_eq!(
            read("tiny\t2\t0\ta\tb\nc\tx\ty\n\tf\tI\tx\n")
                .unwrap_err()
                .line,
            3
        );
        assert_eq!(read("v1\ta\tb\nFOO\tx\ty\n").unwrap_err().line, 2);
        assert!(read("nope\n").is_err());
    }
}
