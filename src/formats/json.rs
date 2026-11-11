//! JSON output format `mappings` version 1 (spec/formats.md#json-output-mappings-version-1).

use crate::model::{ACC_STATIC, Kind, MappingSet};
use serde::{Deserialize, Serialize, Serializer, ser::SerializeMap};
use std::collections::{BTreeMap, HashMap};
use std::io::{self, Read, Write};

pub const FORMAT: &str = "mappings";
pub const FORMAT_VERSION: u32 = 1;
/// JSON Schema of the output, embedded in the binary (`schema` command).
pub const SCHEMA: &str = include_str!("../../spec/mappings.schema.json");

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Configuration {
    pub minecraft_version: String,
    pub launcher: String,
    pub loader: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub loader_version: Option<String>,
}

/// Namespaces whose names the JVM sees, for classes and members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Runtime {
    pub classes: usize,
    pub members: usize,
}

pub struct Export<'a> {
    pub configuration: &'a Configuration,
    pub set: &'a MappingSet,
    pub runtime: Runtime,
    pub named: Option<usize>,
    /// Namespaces exported after `runtime`, in this order.
    pub exported: Vec<usize>,
    pub provenance: &'a serde_json::Value,
    /// `ns0` keys of classes left out of the export.
    pub omit: &'a [String],
}

struct Names<'a>(Vec<(&'a str, &'a str)>);

impl Serialize for Names<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct OutMember<'a> {
    names: Names<'a>,
    descriptor: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    access: Option<u16>,
    #[serde(rename = "static")]
    is_static: bool,
}

#[derive(Serialize)]
struct OutClass<'a> {
    names: Names<'a>,
    #[serde(skip_serializing_if = "Option::is_none")]
    access: Option<u16>,
    #[serde(rename = "super", skip_serializing_if = "Option::is_none")]
    super_name: Option<&'a str>,
    interfaces: Vec<&'a str>,
    fields: Vec<OutMember<'a>>,
    methods: Vec<OutMember<'a>>,
}

#[derive(Serialize)]
struct Aliases<'a> {
    runtime: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    named: Option<&'a str>,
}

#[derive(Serialize)]
struct OutFile<'a> {
    format: &'static str,
    format_version: u32,
    configuration: &'a Configuration,
    namespaces: Vec<&'a str>,
    aliases: Aliases<'a>,
    classes: Vec<OutClass<'a>>,
    provenance: &'a serde_json::Value,
}

fn invalid(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

/// Writes one configuration. Every exported symbol must have a runtime name.
pub fn write(e: &Export, pretty: bool, mut w: impl Write) -> io::Result<()> {
    fn lookup<'x>(map: &HashMap<&str, &'x str>, n: &'x str) -> &'x str {
        map.get(n).copied().unwrap_or(n)
    }
    let set = e.set;
    let ns_name = |i: usize| set.namespaces[i].as_str();
    let class_map = set.class_map(e.runtime.classes);
    let runtime_of = |n| lookup(&class_map, n);
    let aliases = Aliases {
        runtime: if e.runtime.classes == e.runtime.members {
            serde_json::json!(ns_name(e.runtime.classes))
        } else {
            serde_json::json!({ "classes": ns_name(e.runtime.classes), "members": ns_name(e.runtime.members) })
        },
        named: e.named.map(ns_name),
    };
    let mut classes = Vec::with_capacity(set.classes.len());
    let omit: std::collections::HashSet<&str> = e.omit.iter().map(String::as_str).collect();
    for (key, c) in set
        .classes
        .iter()
        .filter(|(k, _)| !omit.contains(k.as_str()))
    {
        let Some(rt) = c.names[e.runtime.classes].as_deref() else {
            return Err(invalid(format!("class {key} has no runtime name")));
        };
        let mut cn = vec![("runtime", rt)];
        cn.extend(
            e.exported
                .iter()
                .filter_map(|&i| c.names[i].as_deref().map(|n| (ns_name(i), n))),
        );
        let mut out = OutClass {
            names: Names(cn),
            access: c.access,
            super_name: c.super_name.as_deref().map(runtime_of),
            interfaces: c
                .interfaces
                .iter()
                .map(|s| runtime_of(s.as_str()))
                .collect(),
            fields: Vec::new(),
            methods: Vec::new(),
        };
        for kind in [Kind::Field, Kind::Method] {
            let mut list = Vec::new();
            for (mk, m) in c.members(kind) {
                let Some(mrt) = m.names[e.runtime.members].as_deref() else {
                    return Err(invalid(format!(
                        "{kind:?} {key}.{}{} has no runtime name",
                        mk.name, mk.desc
                    )));
                };
                let mut mn = vec![("runtime", mrt)];
                mn.extend(
                    e.exported
                        .iter()
                        .filter_map(|&i| m.names[i].as_deref().map(|n| (ns_name(i), n))),
                );
                list.push(OutMember {
                    names: Names(mn),
                    descriptor: MappingSet::desc_in(&class_map, &mk.desc),
                    access: m.access,
                    is_static: m.access.is_some_and(|a| a & ACC_STATIC != 0),
                });
            }
            list.sort_by(|a, b| {
                (a.names.0[0].1, &a.descriptor).cmp(&(b.names.0[0].1, &b.descriptor))
            });
            match kind {
                Kind::Field => out.fields = list,
                Kind::Method => out.methods = list,
            }
        }
        classes.push(out);
    }
    classes.sort_by(|a, b| a.names.0[0].1.cmp(b.names.0[0].1));
    let file = OutFile {
        format: FORMAT,
        format_version: FORMAT_VERSION,
        configuration: e.configuration,
        namespaces: std::iter::once("runtime")
            .chain(e.exported.iter().map(|&i| ns_name(i)))
            .collect(),
        aliases,
        classes,
        provenance: e.provenance,
    };
    if pretty {
        serde_json::to_writer_pretty(&mut w, &file)
    } else {
        serde_json::to_writer(&mut w, &file)
    }
    .map_err(io::Error::from)?;
    w.write_all(b"\n")
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemberRec {
    pub names: BTreeMap<String, String>,
    pub descriptor: String,
    #[allow(dead_code)] // part of the format; verification uses `static`
    pub access: Option<u16>,
    #[serde(rename = "static")]
    pub is_static: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassRec {
    pub names: BTreeMap<String, String>,
    #[allow(dead_code)] // part of the format
    pub access: Option<u16>,
    #[serde(rename = "super")]
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<MemberRec>,
    pub methods: Vec<MemberRec>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub format: String,
    pub format_version: u32,
    pub configuration: Configuration,
    pub namespaces: Vec<String>,
    pub aliases: serde_json::Value,
    pub classes: Vec<ClassRec>,
    pub provenance: serde_json::Value,
}

impl File {
    /// Runtime class name → class record.
    pub fn by_runtime(&self) -> HashMap<&str, &ClassRec> {
        self.classes
            .iter()
            .map(|c| (c.names["runtime"].as_str(), c))
            .collect()
    }
}

/// Reads a (possibly gzipped) output file and checks its format header.
pub fn read(mut r: impl Read) -> Result<File, String> {
    let mut bytes = Vec::new();
    r.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut plain = Vec::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_end(&mut plain)
            .map_err(|e| e.to_string())?;
        bytes = plain;
    }
    let file: File = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if file.format != FORMAT || file.format_version != FORMAT_VERSION {
        return Err(format!(
            "unsupported format {} v{}",
            file.format, file.format_version
        ));
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn sample_set() -> MappingSet {
        let mut s = MappingSet::new(&["official", "intermediary", "mojang"]);
        s.set_class_name("a", 1, "net/minecraft/class_1").unwrap();
        s.set_class_name("a", 2, "net/minecraft/client/Minecraft")
            .unwrap();
        s.class_mut("a").access = Some(33);
        s.class_mut("a").super_name = Some("java/lang/Object".into());
        s.set_member_name("a", Kind::Field, ("b", "La;"), 1, "field_2")
            .unwrap();
        s.member_mut("a", Kind::Field, "b", "La;").access = Some(9);
        s.set_member_name("a", Kind::Method, ("c", "()La;"), 1, "method_3")
            .unwrap();
        s.set_member_name("a", Kind::Method, ("c", "()La;"), 2, "getInstance")
            .unwrap();
        s
    }

    #[test]
    fn writes_runtime_first_sorted_without_nulls() {
        let set = sample_set();
        let cfg = Configuration {
            minecraft_version: "1.21.11".into(),
            launcher: "official".into(),
            loader: "fabric".into(),
            loader_version: Some("0.17.3".into()),
        };
        let prov = serde_json::json!({"generator": "test"});
        let e = Export {
            configuration: &cfg,
            set: &set,
            runtime: Runtime {
                classes: 1,
                members: 1,
            },
            named: Some(2),
            exported: vec![0, 2],
            provenance: &prov,
            omit: &[],
        };
        let mut out = Vec::new();
        write(&e, false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        assert!(text.starts_with(r#"{"format":"mappings","format_version":1,"configuration":{"minecraft_version":"1.21.11","launcher":"official","loader":"fabric","loader_version":"0.17.3"},"namespaces":["runtime","official","mojang"],"aliases":{"runtime":"intermediary","named":"mojang"},"classes":[{"names":{"runtime":"net/minecraft/class_1","official":"a","mojang":"net/minecraft/client/Minecraft"},"access":33,"super":"java/lang/Object""#), "{text}");
        assert!(text.contains(r#"{"names":{"runtime":"field_2","official":"b"},"descriptor":"Lnet/minecraft/class_1;","access":9,"static":true}"#), "{text}");
        assert!(!text.contains("null"));
        let back = read(text.as_bytes()).unwrap();
        assert_eq!(back.classes[0].methods[0].names["mojang"], "getInstance");
        let mut again = Vec::new();
        write(&e, false, &mut again).unwrap();
        assert_eq!(text.as_bytes(), &again[..], "byte-identical output");
    }

    #[test]
    fn missing_runtime_name_is_an_error() {
        let mut set = sample_set();
        set.class_mut("z");
        let cfg = Configuration {
            minecraft_version: "1".into(),
            launcher: "official".into(),
            loader: "vanilla".into(),
            loader_version: None,
        };
        let prov = serde_json::json!({});
        let e = Export {
            configuration: &cfg,
            set: &set,
            runtime: Runtime {
                classes: 1,
                members: 1,
            },
            named: None,
            exported: vec![],
            provenance: &prov,
            omit: &[],
        };
        assert!(write(&e, false, Vec::new()).is_err());
    }

    #[test]
    fn output_matches_the_embedded_schema() {
        let set = sample_set();
        let cfg = Configuration {
            minecraft_version: "1.21.11".into(),
            launcher: "official".into(),
            loader: "forge".into(),
            loader_version: Some("1.21.11-61.2.4".into()),
        };
        let prov = serde_json::json!({
            "generator": "test",
            "sources": [{ "id": "x", "url": "u", "size": 1, "sha256": "0".repeat(64), "license": "CC0-1.0" }],
            "runtime_detection": { "method": "derived", "classes": "mojang", "members": "intermediary" },
            "verification": { "errors": 0, "warnings": 2 },
            "coverage": { "classes": 1.0, "fields": 0.5, "methods": 1.0 }
        });
        // Mixed runtime namespaces (classes and members from different namespaces).
        let e = Export {
            configuration: &cfg,
            set: &set,
            runtime: Runtime {
                classes: 2,
                members: 1,
            },
            named: Some(2),
            exported: vec![0, 1, 2],
            provenance: &prov,
            omit: &[],
        };
        let mut out = Vec::new();
        write(&e, true, &mut out).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let schema: serde_json::Value = serde_json::from_str(SCHEMA).unwrap();
        let validator = jsonschema::validator_for(&schema).unwrap();
        let errors: Vec<String> = validator
            .iter_errors(&value)
            .map(|e| e.to_string())
            .collect();
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(value["aliases"]["runtime"]["classes"], "mojang");
    }
}
