//! Verification (spec/pipeline.md#verification): every exported runtime symbol must exist in
//! the runtime jars with the same descriptor and static flag.

use crate::formats::json::File;
use crate::model::{ACC_STATIC, JarClass, Kind, MappingSet};
use std::collections::{BTreeMap, BTreeSet};

/// `(name, runtime descriptor, static)`
pub type Member = (String, String, bool);

/// Runtime class name → (fields, methods).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Symbols(pub BTreeMap<String, (BTreeSet<Member>, BTreeSet<Member>)>);

impl Symbols {
    /// Symbols of a jar whose names are runtime names (e.g. a Lunar bake).
    pub fn from_jar(jar: &BTreeMap<String, JarClass>) -> Self {
        let set = |list: &[(String, String, u16)]| {
            list.iter()
                .map(|(n, d, a)| (n.clone(), d.clone(), a & ACC_STATIC != 0))
                .collect()
        };
        Symbols(
            jar.iter()
                .map(|(k, c)| (k.clone(), (set(&c.fields), set(&c.methods))))
                .collect(),
        )
    }

    /// Symbols of `set` in runtime names; only symbols with jar data (`access`) are included,
    /// i.e. the client jar remapped into the runtime namespace.
    pub fn from_set(
        set: &MappingSet,
        classes_ns: usize,
        members_ns: usize,
        jar_only: bool,
    ) -> Self {
        let map = set.class_map(classes_ns);
        let mut out = BTreeMap::new();
        for c in set.classes.values() {
            if jar_only && c.access.is_none() {
                continue;
            }
            let Some(rt) = &c.names[classes_ns] else {
                continue;
            };
            let mut sides = (BTreeSet::new(), BTreeSet::new());
            for kind in [Kind::Field, Kind::Method] {
                for (k, m) in c.members(kind) {
                    let (Some(name), false) =
                        (&m.names[members_ns], jar_only && m.access.is_none())
                    else {
                        continue;
                    };
                    let entry = (
                        name.clone(),
                        MappingSet::desc_in(&map, &k.desc),
                        m.access.is_some_and(|a| a & ACC_STATIC != 0),
                    );
                    match kind {
                        Kind::Field => sides.0.insert(entry),
                        Kind::Method => sides.1.insert(entry),
                    };
                }
            }
            out.insert(rt.clone(), sides);
        }
        Symbols(out)
    }

    /// The jar (keyed by `ns0` names) remapped into the runtime namespaces of `set`; names the
    /// set does not cover are kept, as the runtime remapper does.
    pub fn remap_jar(
        jar: &BTreeMap<String, JarClass>,
        set: &MappingSet,
        classes_ns: usize,
        members_ns: usize,
    ) -> Self {
        let map = set.class_map(classes_ns);
        let mut out = BTreeMap::new();
        for (key, jc) in jar {
            let class = set.classes.get(key);
            let rename = |kind: Kind, list: &[(String, String, u16)]| -> BTreeSet<Member> {
                list.iter()
                    .map(|(n, d, a)| {
                        let k = crate::model::MemberKey::new(n, d);
                        let name = class
                            .and_then(|c| c.members(kind).get(&k))
                            .and_then(|m| m.names[members_ns].clone())
                            .unwrap_or_else(|| n.clone());
                        (name, MappingSet::desc_in(&map, d), a & ACC_STATIC != 0)
                    })
                    .collect()
            };
            let rt = map.get(key.as_str()).copied().unwrap_or(key);
            out.insert(
                rt.to_owned(),
                (
                    rename(Kind::Field, &jc.fields),
                    rename(Kind::Method, &jc.methods),
                ),
            );
        }
        Symbols(out)
    }

    /// Symbols declared by an exported file.
    pub fn from_file(file: &File) -> Self {
        let set = |list: &[crate::formats::json::MemberRec]| {
            list.iter()
                .map(|m| {
                    (
                        m.names["runtime"].clone(),
                        m.descriptor.clone(),
                        m.is_static,
                    )
                })
                .collect()
        };
        Symbols(
            file.classes
                .iter()
                .map(|c| {
                    (
                        c.names["runtime"].clone(),
                        (set(&c.fields), set(&c.methods)),
                    )
                })
                .collect(),
        )
    }
}

/// Errors for every exported symbol missing from `runtime`, with a precise diagnostic
/// (wrong name, descriptor or static flag).
pub fn check(exported: &Symbols, runtime: &Symbols) -> Vec<String> {
    let mut errors = Vec::new();
    for (class, (fields, methods)) in &exported.0 {
        let Some((rt_fields, rt_methods)) = runtime.0.get(class) else {
            errors.push(format!("class {class} does not exist at runtime"));
            continue;
        };
        for (kind, mine, theirs) in [
            ("field", fields, rt_fields),
            ("method", methods, rt_methods),
        ] {
            for m @ (name, desc, is_static) in mine {
                if theirs.contains(m) {
                    continue;
                }
                let detail = if theirs.contains(&(name.clone(), desc.clone(), !is_static)) {
                    format!("static is {} at runtime", !is_static)
                } else if let Some(other) = theirs.iter().find(|t| &t.0 == name) {
                    format!("runtime descriptor is {}", other.1)
                } else {
                    "no member of that name".to_owned()
                };
                errors.push(format!(
                    "{kind} {class}.{name}{desc} (static: {is_static}): {detail}"
                ));
            }
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_wrong_name_descriptor_and_static() {
        let mut jar = BTreeMap::new();
        jar.insert(
            "net/minecraft/class_1".to_string(),
            JarClass {
                fields: vec![("field_1".into(), "I".into(), 0)],
                methods: vec![("method_1".into(), "()V".into(), ACC_STATIC)],
                ..JarClass::default()
            },
        );
        let runtime = Symbols::from_jar(&jar);
        let ok = Symbols::from_jar(&jar);
        assert!(check(&ok, &runtime).is_empty());
        let mut bad = Symbols::default();
        let f = BTreeSet::from([
            ("field_1".to_string(), "J".to_string(), false),
            ("field_9".to_string(), "I".to_string(), false),
        ]);
        let m = BTreeSet::from([("method_1".to_string(), "()V".to_string(), false)]);
        bad.0.insert("net/minecraft/class_1".into(), (f, m));
        bad.0
            .insert("net/minecraft/class_2".into(), Default::default());
        let errors = check(&bad, &runtime);
        assert_eq!(errors.len(), 4, "{errors:?}");
        assert!(errors.iter().any(|e| e.contains("runtime descriptor is I")));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("field_9") && e.contains("no member"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("static is true at runtime"))
        );
        assert!(
            errors
                .iter()
                .any(|e| e.contains("class net/minecraft/class_2 does not exist"))
        );
    }
}
