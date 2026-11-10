//! MCP / MCPConfig formats: SRG, CSRG, TSRG v1/v2 and MCP CSV name tables.
//! Fields without a descriptor are keyed with an empty one until the jar resolves it.

use super::{ParseError, err};
use crate::model::{Kind, MappingSet};
use std::collections::HashMap;

fn split_owner(path: &str) -> Option<(&str, &str)> {
    path.rsplit_once('/')
        .filter(|(o, n)| !o.is_empty() && !n.is_empty())
}

fn check(result: Result<(), String>, format: &'static str, line: usize) -> Result<(), ParseError> {
    result.map_err(|message| ParseError {
        format,
        line,
        message,
    })
}

/// SRG (`PK:`/`CL:`/`FD:`/`MD:`), namespaces `official` → `srg`.
pub fn read_srg(text: &str) -> Result<MappingSet, ParseError> {
    const F: &str = "srg";
    let mut set = MappingSet::new(&["official", "srg"]);
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        let cols: Vec<&str> = raw.split_whitespace().collect();
        match cols.as_slice() {
            [] => {}
            ["PK:", _, _] => {}
            ["CL:", obf, srg] => check(set.set_class_name(obf, 1, srg), F, line)?,
            ["FD:", obf, srg] => {
                let (Some((owner, name)), Some((_, new))) = (split_owner(obf), split_owner(srg))
                else {
                    return err(F, line, "malformed field");
                };
                check(
                    set.set_member_name(owner, Kind::Field, (name, ""), 1, new),
                    F,
                    line,
                )?;
            }
            ["MD:", obf, desc, srg, _] => {
                let (Some((owner, name)), Some((_, new))) = (split_owner(obf), split_owner(srg))
                else {
                    return err(F, line, "malformed method");
                };
                check(
                    set.set_member_name(owner, Kind::Method, (name, desc), 1, new),
                    F,
                    line,
                )?;
            }
            _ => return err(F, line, format!("unknown entry {raw:?}")),
        }
    }
    Ok(set)
}

#[allow(dead_code)] // no supported era publishes CSRG today; kept for sources that do
/// CSRG (`class new`, `owner field new`, `owner method desc new`), `official` → `srg`.
pub fn read_csrg(text: &str) -> Result<MappingSet, ParseError> {
    const F: &str = "csrg";
    let mut set = MappingSet::new(&["official", "srg"]);
    for (i, raw) in text.lines().enumerate() {
        let line = i + 1;
        if raw.starts_with('#') {
            continue;
        }
        match raw.split_whitespace().collect::<Vec<_>>().as_slice() {
            [] => {}
            [obf, new] => check(set.set_class_name(obf, 1, new), F, line)?,
            [owner, name, new] => check(
                set.set_member_name(owner, Kind::Field, (name, ""), 1, new),
                F,
                line,
            )?,
            [owner, name, desc, new] => check(
                set.set_member_name(owner, Kind::Method, (name, desc), 1, new),
                F,
                line,
            )?,
            _ => return err(F, line, format!("unknown entry {raw:?}")),
        }
    }
    Ok(set)
}

/// TSRG v1 (`official` → `srg`) or TSRG2 (namespaces from the header). Parameter lines and
/// `static` markers are accepted and ignored: access flags come from the jar.
pub fn read_tsrg(text: &str) -> Result<MappingSet, ParseError> {
    const F: &str = "tsrg";
    let mut lines = text.lines().enumerate().peekable();
    let v2: Option<Vec<&str>> = match lines.peek() {
        Some((_, h)) if h.starts_with("tsrg2 ") => Some(h.split_whitespace().skip(1).collect()),
        _ => None,
    };
    if v2.is_some() {
        lines.next();
    }
    let ns: Vec<&str> = v2.clone().unwrap_or_else(|| vec!["official", "srg"]);
    if ns.len() < 2 {
        return err(F, 1, "tsrg2 header needs at least two namespaces");
    }
    let n = ns.len();
    let mut set = MappingSet::new(&ns);
    let mut class: Option<String> = None;
    let names_into =
        |set: &mut MappingSet, line, target: Option<(Kind, &str, &str)>, names: &[&str]| {
            for (i, name) in names.iter().enumerate().skip(1) {
                let r = match target {
                    None => set.set_class_name(names[0], i, name),
                    Some((kind, owner, desc)) => {
                        set.set_member_name(owner, kind, (names[0], desc), i, name)
                    }
                };
                check(r, F, line)?;
            }
            Ok::<(), ParseError>(())
        };
    for (i, raw) in lines {
        let line = i + 1;
        if raw.trim().is_empty() || raw.starts_with('#') {
            continue;
        }
        let depth = raw.len() - raw.trim_start_matches('\t').len();
        let cols: Vec<&str> = raw.split_whitespace().collect();
        match depth {
            0 if cols.len() == n => {
                names_into(&mut set, line, None, &cols)?;
                class = Some(cols[0].to_owned());
            }
            1 => {
                let Some(owner) = class.clone() else {
                    return err(F, line, "member outside a class");
                };
                let with_desc = cols.len() == n + 1;
                if !with_desc && cols.len() != n {
                    return err(F, line, format!("expected {n} names"));
                }
                let (kind, desc, names) = match with_desc {
                    true if cols[1].starts_with('(') => {
                        (Kind::Method, cols[1], [&cols[..1], &cols[2..]].concat())
                    }
                    true => (Kind::Field, cols[1], [&cols[..1], &cols[2..]].concat()),
                    false => (Kind::Field, "", cols.clone()),
                };
                names_into(&mut set, line, Some((kind, &owner, desc)), &names)?;
            }
            2 if v2.is_some() => {} // `static` marker or parameter
            _ => return err(F, line, format!("unexpected line {raw:?}")),
        }
    }
    Ok(set)
}

/// MCP `fields.csv` / `methods.csv`: `searge,name,side,desc` → `searge -> name`.
pub fn read_mcp_csv(text: &str) -> Result<HashMap<String, String>, ParseError> {
    const F: &str = "mcp csv";
    let mut lines = text.lines().enumerate();
    match lines.next() {
        Some((_, h)) if h.starts_with("searge,name") => {}
        _ => return err(F, 1, "expected header 'searge,name,...'"),
    }
    let mut out = HashMap::new();
    for (i, raw) in lines {
        if raw.is_empty() {
            continue;
        }
        let mut cols = raw.splitn(3, ',');
        match (cols.next(), cols.next()) {
            (Some(srg), Some(name))
                if !srg.is_empty() && !name.is_empty() && !name.contains('"') =>
            {
                out.insert(srg.to_owned(), name.to_owned());
            }
            _ => return err(F, i + 1, "malformed row"),
        }
    }
    Ok(out)
}

/// Adds namespace `mcp` over `srg`: class names are the SRG ones (MCP class names, ≤ 1.16.5),
/// member names come from the CSV tables; SRG members without an MCP name stay unnamed.
pub fn apply_mcp(
    set: &mut MappingSet,
    fields: &HashMap<String, String>,
    methods: &HashMap<String, String>,
) -> Result<(), String> {
    let srg = set.ns("srg").ok_or("no srg namespace")?;
    let mcp = set.add_namespace("mcp");
    for class in set.classes.values_mut() {
        class.names[mcp] = class.names[srg].clone();
        for (members, table) in [(&mut class.fields, fields), (&mut class.methods, methods)] {
            for m in members.values_mut() {
                m.names[mcp] = match m.names[srg].as_deref() {
                    Some(s) if s.starts_with("func_") || s.starts_with("field_") => {
                        table.get(s).cloned()
                    }
                    other => other.map(str::to_owned), // names SRG did not rename (e.g. `<init>`)
                };
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MemberKey;

    #[test]
    fn srg_1_7_10_style() {
        let s = read_srg(
            "PK: . net/minecraft/src\nCL: bao net/minecraft/client/Minecraft\n\
FD: bao/M net/minecraft/client/Minecraft/field_71432_P\n\
MD: bao/B ()Lbao; net/minecraft/client/Minecraft/func_71410_x ()Lnet/minecraft/client/Minecraft;\n",
        )
        .unwrap();
        let c = &s.classes["bao"];
        assert_eq!(
            c.fields[&MemberKey::new("M", "")].names[1].as_deref(),
            Some("field_71432_P")
        );
        assert_eq!(
            c.methods[&MemberKey::new("B", "()Lbao;")].names[1].as_deref(),
            Some("func_71410_x")
        );
        assert_eq!(read_srg("XX: a b\n").unwrap_err().line, 1);
    }

    #[test]
    fn csrg() {
        let s = read_csrg(
            "# c\nbao net/minecraft/client/Minecraft\nbao M field_1\nbao B ()Lbao; func_1\n",
        )
        .unwrap();
        assert_eq!(s.classes["bao"].methods.len(), 1);
        assert_eq!(read_csrg("a b c d e\n").unwrap_err().line, 1);
    }

    #[test]
    fn tsrg_v1_1_16_style() {
        let s = read_tsrg(
            "djz net/minecraft/client/Minecraft\n\tb field_142025_a\n\tc ()Ldjz; func_71410_x\n",
        )
        .unwrap();
        let c = &s.classes["djz"];
        assert!(c.fields.contains_key(&MemberKey::new("b", "")));
        assert_eq!(
            c.methods[&MemberKey::new("c", "()Ldjz;")].names[1].as_deref(),
            Some("func_71410_x")
        );
    }

    #[test]
    fn tsrg2_1_20_1_style() {
        let text = "tsrg2 obf srg id\nenn net/minecraft/src/C_3391_ 3391\n\
\ta f_90981_ 90981\n\tb Lenn; f_1_ 1\n\tc ()Lenn; m_91087_ 91087\n\t\tstatic\n\td (I)V m_2_ 2\n\t\t0 o p_1_ 7\n";
        let s = read_tsrg(text).unwrap();
        assert_eq!(s.namespaces, ["obf", "srg", "id"]);
        let c = &s.classes["enn"];
        assert_eq!(c.names[1].as_deref(), Some("net/minecraft/src/C_3391_"));
        assert!(c.fields.contains_key(&MemberKey::new("a", "")));
        assert!(c.fields.contains_key(&MemberKey::new("b", "Lenn;")));
        assert_eq!(
            c.methods[&MemberKey::new("c", "()Lenn;")].names[1].as_deref(),
            Some("m_91087_")
        );
        assert_eq!(
            read_tsrg("tsrg2 obf srg\na b\n\tx y z w\n")
                .unwrap_err()
                .line,
            3
        );
    }

    #[test]
    fn mcp_csv_applies_names() {
        let methods = read_mcp_csv(
            "searge,name,side,desc\nfunc_71410_x,getMinecraft,0,\"Returns, the instance\"\n",
        )
        .unwrap();
        let fields = read_mcp_csv("searge,name,side,desc\n").unwrap();
        let mut s = read_tsrg(
            "djz net/minecraft/client/Minecraft\n\tc ()Ldjz; func_71410_x\n\td ()V func_1_a\n",
        )
        .unwrap();
        apply_mcp(&mut s, &fields, &methods).unwrap();
        let mcp = s.ns("mcp").unwrap();
        let c = &s.classes["djz"];
        assert_eq!(
            c.names[mcp].as_deref(),
            Some("net/minecraft/client/Minecraft")
        );
        assert_eq!(
            c.methods[&MemberKey::new("c", "()Ldjz;")].names[mcp].as_deref(),
            Some("getMinecraft")
        );
        assert_eq!(c.methods[&MemberKey::new("d", "()V")].names[mcp], None);
        assert!(read_mcp_csv("a,b\n").is_err());
    }
}
