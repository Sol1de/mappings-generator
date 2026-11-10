//! Multi-namespace mapping model (spec/model.md). No I/O.

mod desc;

pub use desc::{descriptor_classes, java_type_to_descriptor, remap_descriptor, valid_descriptor};

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};

pub const ACC_PRIVATE: u16 = 0x0002;
pub const ACC_STATIC: u16 = 0x0008;

/// Names of one symbol, indexed like [`MappingSet::namespaces`].
pub type Names = Vec<Option<String>>;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MemberKey {
    pub name: String,
    /// Descriptor in `ns0`. Empty only while a field descriptor is unknown (SRG/TSRG v1
    /// sources) until [`MappingSet::attach_jar`] resolves it.
    pub desc: String,
}

impl MemberKey {
    pub fn new(name: impl Into<String>, desc: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            desc: desc.into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Member {
    pub names: Names,
    pub access: Option<u16>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Class {
    pub names: Names,
    pub access: Option<u16>,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: BTreeMap<MemberKey, Member>,
    pub methods: BTreeMap<MemberKey, Member>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Field,
    Method,
}

/// Class data read from a jar, keyed by its internal name in `ns0`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JarClass {
    pub access: u16,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    /// `(name, descriptor, access)`.
    pub fields: Vec<(String, String, u16)>,
    pub methods: Vec<(String, String, u16)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MappingSet {
    /// `namespaces[0]` (`ns0`) keys classes and members and holds descriptors.
    pub namespaces: Vec<String>,
    pub classes: BTreeMap<String, Class>,
}

/// How [`MappingSet::complete`] fills missing names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Complete<'a> {
    /// The namespace is produced by a remapper that keeps unmapped names: every missing
    /// name is the `ns0` name (exact runtime behaviour, not a guess).
    Remap,
    /// Only names proven unobfuscated: `<init>`/`<clinit>`, or symbols whose name in one of
    /// these namespaces equals their `ns0` name.
    Proven(&'a [usize]),
}

fn set_name(
    names: &mut Names,
    ns: usize,
    value: &str,
    what: &dyn Fn() -> String,
) -> Result<(), String> {
    match &names[ns] {
        Some(old) if old != value => Err(format!("{}: {old:?} vs {value:?}", what())),
        Some(_) => Ok(()),
        None => {
            names[ns] = Some(value.to_owned());
            Ok(())
        }
    }
}

fn check_class_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains(|c: char| matches!(c, '.' | ';' | '[') || c.is_whitespace())
        && name.split('/').all(|s| !s.is_empty())
}

fn check_member_name(name: &str, kind: Kind) -> bool {
    if kind == Kind::Method && matches!(name, "<init>" | "<clinit>") {
        return true;
    }
    !name.is_empty() && !name.contains(['.', ';', '[', '/', '<', '>'])
}

impl Class {
    pub fn members(&self, kind: Kind) -> &BTreeMap<MemberKey, Member> {
        match kind {
            Kind::Field => &self.fields,
            Kind::Method => &self.methods,
        }
    }

    pub fn members_mut(&mut self, kind: Kind) -> &mut BTreeMap<MemberKey, Member> {
        match kind {
            Kind::Field => &mut self.fields,
            Kind::Method => &mut self.methods,
        }
    }
}

impl MappingSet {
    pub fn new(namespaces: &[&str]) -> Self {
        Self {
            namespaces: namespaces.iter().map(|s| s.to_string()).collect(),
            classes: BTreeMap::new(),
        }
    }

    pub fn ns(&self, name: &str) -> Option<usize> {
        self.namespaces.iter().position(|n| n == name)
    }

    fn empty_names(&self) -> Names {
        vec![None; self.namespaces.len()]
    }

    /// Adds a namespace (no names yet) and returns its index; existing ones are reused.
    pub fn add_namespace(&mut self, name: &str) -> usize {
        if let Some(i) = self.ns(name) {
            return i;
        }
        self.namespaces.push(name.to_owned());
        for class in self.classes.values_mut() {
            class.names.push(None);
            for m in class.fields.values_mut().chain(class.methods.values_mut()) {
                m.names.push(None);
            }
        }
        self.namespaces.len() - 1
    }

    pub fn rename_ns(&mut self, old: &str, new: &str) -> bool {
        match self.ns(old) {
            Some(i) if self.ns(new).is_none() => {
                self.namespaces[i] = new.to_owned();
                true
            }
            _ => false,
        }
    }

    /// Removes a namespace other than `ns0`.
    pub fn drop_ns(&mut self, name: &str) -> bool {
        let Some(i) = self.ns(name).filter(|&i| i > 0) else {
            return false;
        };
        self.namespaces.remove(i);
        for class in self.classes.values_mut() {
            class.names.remove(i);
            for m in class.fields.values_mut().chain(class.methods.values_mut()) {
                m.names.remove(i);
            }
        }
        true
    }

    /// The class keyed by `ns0` name `key`, created if missing.
    pub fn class_mut(&mut self, key: &str) -> &mut Class {
        let names = self.empty_names();
        self.classes.entry(key.to_owned()).or_insert_with(|| {
            let mut names = names;
            names[0] = Some(key.to_owned());
            Class {
                names,
                ..Class::default()
            }
        })
    }

    /// The member `name`/`desc` (in `ns0`) of class `owner`, both created if missing.
    pub fn member_mut(&mut self, owner: &str, kind: Kind, name: &str, desc: &str) -> &mut Member {
        let names = self.empty_names();
        self.class_mut(owner)
            .members_mut(kind)
            .entry(MemberKey::new(name, desc))
            .or_insert_with(|| {
                let mut names = names;
                names[0] = Some(name.to_owned());
                Member {
                    names,
                    access: None,
                }
            })
    }

    /// Sets a name, reporting a conflict if another non-empty name is already present.
    pub fn set_class_name(&mut self, key: &str, ns: usize, name: &str) -> Result<(), String> {
        let class = self.class_mut(key);
        set_name(&mut class.names, ns, name, &|| format!("class {key}"))
    }

    pub fn set_member_name(
        &mut self,
        owner: &str,
        kind: Kind,
        key: (&str, &str),
        ns: usize,
        name: &str,
    ) -> Result<(), String> {
        let member = self.member_mut(owner, kind, key.0, key.1);
        set_name(&mut member.names, ns, name, &|| {
            format!("{kind:?} {owner}.{}{}", key.0, key.1)
        })
    }

    /// `ns0` class name -> class name in `ns`, for classes named in both.
    pub fn class_map(&self, ns: usize) -> HashMap<&str, &str> {
        self.classes
            .iter()
            .filter_map(|(k, c)| c.names[ns].as_deref().map(|n| (k.as_str(), n)))
            .collect()
    }

    /// Remaps an `ns0` descriptor into namespace `ns` with a map from [`Self::class_map`].
    pub fn desc_in(map: &HashMap<&str, &str>, desc: &str) -> String {
        remap_descriptor(desc, |n| map.get(n).copied())
    }

    /// Joins `other` (same `ns0`) into `self`. Every conflicting name is reported.
    pub fn merge(&mut self, other: &MappingSet) -> Result<(), Vec<String>> {
        if self.namespaces[0] != other.namespaces[0] {
            return Err(vec![format!(
                "cannot merge sets keyed by {} and {}",
                self.namespaces[0], other.namespaces[0]
            )]);
        }
        let index: Vec<usize> = other
            .namespaces
            .iter()
            .map(|n| self.add_namespace(n))
            .collect();
        let mut conflicts = Vec::new();
        for (key, theirs) in &other.classes {
            let ours = self.class_mut(key);
            for (i, name) in theirs.names.iter().enumerate() {
                if let Some(name) = name
                    && let Err(e) =
                        set_name(&mut ours.names, index[i], name, &|| format!("class {key}"))
                {
                    conflicts.push(e);
                }
            }
            ours.access = ours.access.or(theirs.access);
            if ours.super_name.is_none() {
                ours.super_name.clone_from(&theirs.super_name);
            }
            if ours.interfaces.is_empty() {
                ours.interfaces.clone_from(&theirs.interfaces);
            }
            for kind in [Kind::Field, Kind::Method] {
                for (mk, m) in theirs.members(kind) {
                    let width = ours.names.len();
                    let target =
                        ours.members_mut(kind)
                            .entry(mk.clone())
                            .or_insert_with(|| Member {
                                names: vec![None; width],
                                access: None,
                            });
                    target.access = target.access.or(m.access);
                    for (i, name) in m.names.iter().enumerate() {
                        if let Some(name) = name {
                            let what = || format!("{kind:?} {key}.{}{}", mk.name, mk.desc);
                            if let Err(e) = set_name(&mut target.names, index[i], name, &what) {
                                conflicts.push(e);
                            }
                        }
                    }
                }
            }
        }
        if conflicts.is_empty() {
            Ok(())
        } else {
            Err(conflicts)
        }
    }

    #[allow(dead_code)] // model operation of the spec; no current era needs it
    /// `self: X→Y`, `other: Y→Z` (keyed by Y): adds `other`'s namespaces to `self` through
    /// the shared namespace `y`. Entries whose `y` name is unknown to `other` are returned.
    pub fn compose(&mut self, other: &MappingSet) -> Result<Vec<String>, String> {
        let y_name = &other.namespaces[0];
        let y = self
            .ns(y_name)
            .ok_or_else(|| format!("shared namespace {y_name} missing"))?;
        let index: Vec<usize> = other
            .namespaces
            .iter()
            .map(|n| self.add_namespace(n))
            .collect();
        let y_map: HashMap<String, String> = self
            .class_map(y)
            .into_iter()
            .map(|(k, v)| (k.to_owned(), v.to_owned()))
            .collect();
        let mut losses = Vec::new();
        for (key, class) in &mut self.classes {
            let Some(y_class) = class.names[y].clone() else {
                continue;
            };
            let Some(theirs) = other.classes.get(&y_class) else {
                losses.push(format!("class {key} ({y_class})"));
                continue;
            };
            for (i, n) in theirs.names.iter().enumerate().skip(1) {
                if class.names[index[i]].is_none() {
                    class.names[index[i]].clone_from(n);
                }
            }
            for kind in [Kind::Field, Kind::Method] {
                for (mk, m) in class.members_mut(kind) {
                    let Some(y_member) = m.names[y].clone() else {
                        continue;
                    };
                    let y_desc = remap_descriptor(&mk.desc, |n| y_map.get(n).map(String::as_str));
                    let found = theirs
                        .members(kind)
                        .get(&MemberKey::new(&y_member, &y_desc))
                        .or_else(|| {
                            // Fields from descriptor-less sources are keyed with an empty descriptor.
                            (kind == Kind::Field)
                                .then(|| theirs.fields.get(&MemberKey::new(&y_member, "")))
                                .flatten()
                        });
                    match found {
                        Some(t) => {
                            for (i, n) in t.names.iter().enumerate().skip(1) {
                                if m.names[index[i]].is_none() {
                                    m.names[index[i]].clone_from(n);
                                }
                            }
                        }
                        None => losses.push(format!(
                            "{kind:?} {key}.{}{} ({y_member})",
                            mk.name, mk.desc
                        )),
                    }
                }
            }
        }
        Ok(losses)
    }

    /// Re-keys the set by namespace `name`. Symbols without a name there are returned as losses.
    pub fn reorder(&self, name: &str) -> Result<(MappingSet, Vec<String>), String> {
        let ns = self
            .ns(name)
            .ok_or_else(|| format!("unknown namespace {name}"))?;
        let order: Vec<usize> = std::iter::once(ns)
            .chain((0..self.namespaces.len()).filter(|&i| i != ns))
            .collect();
        let pick = |names: &Names| order.iter().map(|&i| names[i].clone()).collect::<Names>();
        let map = self.class_map(ns);
        let mut out = MappingSet {
            namespaces: order.iter().map(|&i| self.namespaces[i].clone()).collect(),
            classes: BTreeMap::new(),
        };
        let mut losses = Vec::new();
        for (key, class) in &self.classes {
            let Some(new_key) = &class.names[ns] else {
                losses.push(format!("class {key}"));
                continue;
            };
            let mut c = Class {
                names: pick(&class.names),
                access: class.access,
                super_name: class
                    .super_name
                    .as_deref()
                    .map(|s| map.get(s).copied().unwrap_or(s).to_owned()),
                interfaces: class
                    .interfaces
                    .iter()
                    .map(|s| map.get(s.as_str()).copied().unwrap_or(s).to_owned())
                    .collect(),
                ..Class::default()
            };
            for kind in [Kind::Field, Kind::Method] {
                for (mk, m) in class.members(kind) {
                    match &m.names[ns] {
                        Some(n) => {
                            let k = MemberKey::new(n, Self::desc_in(&map, &mk.desc));
                            c.members_mut(kind).insert(
                                k,
                                Member {
                                    names: pick(&m.names),
                                    access: m.access,
                                },
                            );
                        }
                        None => losses.push(format!("{kind:?} {key}.{}{}", mk.name, mk.desc)),
                    }
                }
            }
            if out.classes.insert(new_key.clone(), c).is_some() {
                return Err(format!("duplicate class {new_key} in {name}"));
            }
        }
        Ok((out, losses))
    }

    /// Adds jar classes and members (keyed by `ns0` names), sets access and hierarchy, and
    /// resolves unknown field descriptors. Returns fields whose descriptor stays ambiguous.
    pub fn attach_jar(&mut self, jar: &BTreeMap<String, JarClass>) -> Vec<String> {
        let mut problems = Vec::new();
        for (name, jc) in jar {
            let width = self.namespaces.len();
            let class = self.class_mut(name);
            class.access = Some(jc.access);
            class.super_name.clone_from(&jc.super_name);
            class.interfaces.clone_from(&jc.interfaces);
            let unknown: Vec<MemberKey> = class
                .fields
                .keys()
                .filter(|k| k.desc.is_empty())
                .cloned()
                .collect();
            for key in unknown {
                let candidates: Vec<_> = jc.fields.iter().filter(|f| f.0 == key.name).collect();
                let member = class.fields.remove(&key).expect("key listed above");
                match candidates.as_slice() {
                    [(n, d, _)] if !class.fields.contains_key(&MemberKey::new(n, d)) => {
                        class.fields.insert(MemberKey::new(n, d), member);
                    }
                    // Not in this jar (e.g. a server-only field of a joined table): not a symbol here.
                    [] => {}
                    _ => problems.push(format!(
                        "field {name}.{}: {} candidates in jar",
                        key.name,
                        candidates.len()
                    )),
                }
            }
            for (kind, list) in [(Kind::Field, &jc.fields), (Kind::Method, &jc.methods)] {
                for (n, d, access) in list {
                    let m = class
                        .members_mut(kind)
                        .entry(MemberKey::new(n, d))
                        .or_insert_with(|| {
                            let mut names = vec![None; width];
                            names[0] = Some(n.clone());
                            Member {
                                names,
                                access: None,
                            }
                        });
                    m.access = Some(*access);
                }
            }
        }
        problems
    }

    /// Ancestors of `key` (super classes and interfaces known to the set), nearest first.
    fn ancestors(&self, key: &str) -> Vec<&str> {
        let mut seen = HashSet::new();
        let mut queue: VecDeque<&str> = VecDeque::from([key]);
        let mut out = Vec::new();
        while let Some(k) = queue.pop_front() {
            let Some(c) = self.classes.get(k) else {
                continue;
            };
            for parent in c.super_name.iter().chain(&c.interfaces) {
                if seen.insert(parent.as_str()) {
                    out.push(parent.as_str());
                    queue.push_back(parent);
                }
            }
        }
        out
    }

    /// Copies method names from overridden ancestor methods (same `ns0` name and descriptor)
    /// to overriding methods that lack them. Returns conflicting inherited names.
    pub fn propagate(&mut self) -> Vec<String> {
        let inheritable = |m: &Member| m.access.is_none_or(|a| a & (ACC_PRIVATE | ACC_STATIC) == 0);
        let mut updates = Vec::new();
        let mut conflicts = Vec::new();
        for (key, class) in &self.classes {
            let ancestors = self.ancestors(key);
            for (mk, m) in &class.methods {
                if mk.name.starts_with('<')
                    || !inheritable(m)
                    || m.names.iter().all(Option::is_some)
                {
                    continue;
                }
                for ns in 1..self.namespaces.len() {
                    if m.names[ns].is_some() {
                        continue;
                    }
                    let found: BTreeSet<&str> = ancestors
                        .iter()
                        .filter_map(|a| self.classes.get(*a)?.methods.get(mk))
                        .filter(|am| inheritable(am))
                        .filter_map(|am| am.names[ns].as_deref())
                        .collect();
                    match found.len() {
                        0 => {}
                        1 => updates.push((
                            key.clone(),
                            mk.clone(),
                            ns,
                            found.first().unwrap().to_string(),
                        )),
                        _ => conflicts.push(format!(
                            "method {key}.{}{} inherits {found:?}",
                            mk.name, mk.desc
                        )),
                    }
                }
            }
        }
        for (key, mk, ns, name) in updates {
            self.classes
                .get_mut(&key)
                .unwrap()
                .methods
                .get_mut(&mk)
                .unwrap()
                .names[ns] = Some(name);
        }
        conflicts
    }

    /// Fills missing names of namespace `ns` with those of namespace `from`.
    pub fn fill_from(&mut self, ns: usize, from: usize) {
        let fill = |names: &mut Names| {
            if names[ns].is_none() {
                names[ns] = names[from].clone();
            }
        };
        for class in self.classes.values_mut() {
            fill(&mut class.names);
            for m in class.fields.values_mut().chain(class.methods.values_mut()) {
                fill(&mut m.names);
            }
        }
    }

    /// Fills missing names of namespace `ns` from `ns0` according to `policy`.
    pub fn complete(&mut self, ns: usize, policy: Complete) {
        let fill = |names: &mut Names, special: bool| {
            if names[ns].is_some() {
                return;
            }
            let proven = match policy {
                Complete::Remap => true,
                Complete::Proven(witnesses) => {
                    special || witnesses.iter().any(|&w| names[w] == names[0])
                }
            };
            if proven {
                names[ns] = names[0].clone();
            }
        };
        for class in self.classes.values_mut() {
            fill(&mut class.names, false);
            for m in class.fields.values_mut() {
                fill(&mut m.names, false);
            }
            for (k, m) in &mut class.methods {
                fill(&mut m.names, k.name.starts_with('<'));
            }
        }
    }

    /// Checks the model invariants; returns every violation found.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        for ns in 0..self.namespaces.len() {
            let ns_name = &self.namespaces[ns];
            let map = self.class_map(ns);
            let mut class_names = HashSet::new();
            for (key, class) in &self.classes {
                let Some(name) = &class.names[ns] else {
                    continue;
                };
                if !check_class_name(name) {
                    errors.push(format!("{ns_name}: invalid class name {name:?}"));
                }
                if !class_names.insert(name.as_str()) {
                    errors.push(format!("{ns_name}: duplicate class {name}"));
                }
                for kind in [Kind::Field, Kind::Method] {
                    let mut seen = HashSet::new();
                    for (mk, m) in class.members(kind) {
                        let Some(mname) = &m.names[ns] else { continue };
                        if !check_member_name(mname, kind) {
                            errors.push(format!("{ns_name}: invalid member name {name}.{mname:?}"));
                        }
                        if ns == 0 && !valid_descriptor(&mk.desc, kind == Kind::Method) {
                            errors
                                .push(format!("invalid descriptor {key}.{}{:?}", mk.name, mk.desc));
                        }
                        if !seen.insert((mname.as_str(), Self::desc_in(&map, &mk.desc))) {
                            errors.push(format!(
                                "{ns_name}: duplicate {kind:?} {name}.{mname}{}",
                                mk.desc
                            ));
                        }
                        if ns == 0 && ns_name == "official" {
                            for c in descriptor_classes(&mk.desc) {
                                // Obfuscated classes live in the default package; JDK and
                                // library classes never do. (Runtime-keyed sets, e.g. Lunar's,
                                // may reference odd names the runtime itself contains.)
                                if !c.contains('/') && !self.classes.contains_key(c) {
                                    errors.push(format!(
                                        "unmapped class {c} referenced by {key}.{}",
                                        mk.name
                                    ));
                                }
                            }
                        }
                    }
                }
            }
        }
        errors
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> MappingSet {
        let mut s = MappingSet::new(&["official", "named"]);
        s.set_class_name("a", 1, "pkg/Client").unwrap();
        s.set_class_name("b", 1, "pkg/Entity").unwrap();
        s.set_member_name("a", Kind::Field, ("c", "Lb;"), 1, "player")
            .unwrap();
        s.set_member_name("a", Kind::Method, ("d", "([[Lb;I)La;"), 1, "find")
            .unwrap();
        s.set_member_name("a", Kind::Method, ("<init>", "()V"), 1, "<init>")
            .unwrap();
        s
    }

    #[test]
    fn merge_reports_conflicts_and_joins() {
        let mut a = sample();
        let mut b = MappingSet::new(&["official", "intermediary"]);
        b.set_class_name("a", 1, "class_1").unwrap();
        b.set_member_name("a", Kind::Field, ("c", "Lb;"), 1, "field_1")
            .unwrap();
        a.merge(&b).unwrap();
        assert_eq!(a.namespaces, ["official", "named", "intermediary"]);
        assert_eq!(
            a.classes["a"].fields[&MemberKey::new("c", "Lb;")].names[2].as_deref(),
            Some("field_1")
        );
        let mut c = MappingSet::new(&["official", "named"]);
        c.set_class_name("a", 1, "pkg/Other").unwrap();
        c.set_class_name("b", 1, "pkg/Else").unwrap();
        let err = a.merge(&c).unwrap_err();
        assert_eq!(err.len(), 2);
    }

    #[test]
    fn reorder_remaps_nested_array_descriptors() {
        let (r, losses) = sample().reorder("named").unwrap();
        assert!(losses.is_empty());
        let client = &r.classes["pkg/Client"];
        assert!(
            client
                .methods
                .contains_key(&MemberKey::new("find", "([[Lpkg/Entity;I)Lpkg/Client;"))
        );
        assert!(
            client
                .methods
                .contains_key(&MemberKey::new("<init>", "()V"))
        );
        assert_eq!(r.namespaces, ["named", "official"]);
    }

    #[test]
    fn compose_with_holes_reports_losses() {
        let mut xy = MappingSet::new(&["official", "srg"]);
        xy.set_class_name("a", 1, "C_1_").unwrap();
        xy.set_class_name("b", 1, "C_2_").unwrap();
        xy.set_member_name("a", Kind::Method, ("d", "(Lb;)V"), 1, "m_1_")
            .unwrap();
        xy.set_member_name("a", Kind::Method, ("e", "()V"), 1, "m_2_")
            .unwrap();
        let mut yz = MappingSet::new(&["srg", "mcp"]);
        yz.set_class_name("C_1_", 1, "Client").unwrap();
        yz.set_member_name("C_1_", Kind::Method, ("m_1_", "(LC_2_;)V"), 1, "tick")
            .unwrap();
        let losses = xy.compose(&yz).unwrap();
        assert_eq!(losses.len(), 2, "{losses:?}");
        let mcp = xy.ns("mcp").unwrap();
        assert_eq!(
            xy.classes["a"].methods[&MemberKey::new("d", "(Lb;)V")].names[mcp].as_deref(),
            Some("tick")
        );
    }

    #[test]
    fn attach_propagate_complete() {
        let mut s = MappingSet::new(&["official", "named"]);
        s.set_class_name("p", 1, "Parent").unwrap();
        s.set_member_name("p", Kind::Method, ("a", "()V"), 1, "run")
            .unwrap();
        s.class_mut("c").fields.insert(
            MemberKey::new("f", ""),
            Member {
                names: vec![Some("f".into()), Some("x".into())],
                access: None,
            },
        );
        let mut jar = BTreeMap::new();
        jar.insert(
            "p".into(),
            JarClass {
                methods: vec![("a".into(), "()V".into(), 1)],
                ..JarClass::default()
            },
        );
        jar.insert(
            "c".into(),
            JarClass {
                super_name: Some("p".into()),
                fields: vec![("f".into(), "I".into(), 2)],
                methods: vec![
                    ("a".into(), "()V".into(), 1),
                    ("<init>".into(), "()V".into(), 1),
                    ("lambda$0".into(), "()V".into(), 0x1008),
                ],
                ..JarClass::default()
            },
        );
        assert!(s.attach_jar(&jar).is_empty());
        assert_eq!(
            s.classes["c"].fields[&MemberKey::new("f", "I")].access,
            Some(2)
        );
        assert!(s.propagate().is_empty());
        assert_eq!(
            s.classes["c"].methods[&MemberKey::new("a", "()V")].names[1].as_deref(),
            Some("run")
        );
        s.complete(1, Complete::Proven(&[]));
        assert_eq!(
            s.classes["c"].methods[&MemberKey::new("<init>", "()V")].names[1].as_deref(),
            Some("<init>")
        );
        assert_eq!(
            s.classes["c"].methods[&MemberKey::new("lambda$0", "()V")].names[1],
            None
        );
        assert_eq!(s.classes["c"].names[1], None);
        s.complete(1, Complete::Remap);
        assert_eq!(s.classes["c"].names[1].as_deref(), Some("c"));
        assert!(s.validate().is_empty(), "{:?}", s.validate());
    }

    #[test]
    fn fill_from_keeps_existing_names() {
        let mut s = sample();
        s.add_namespace("srg");
        s.set_member_name("a", Kind::Method, ("d", "([[Lb;I)La;"), 2, "func_1")
            .unwrap();
        s.set_member_name("a", Kind::Field, ("c", "Lb;"), 2, "field_1")
            .unwrap();
        s.classes
            .get_mut("a")
            .unwrap()
            .fields
            .get_mut(&MemberKey::new("c", "Lb;"))
            .unwrap()
            .names[1] = None;
        s.fill_from(1, 2);
        let a = &s.classes["a"];
        assert_eq!(
            a.fields[&MemberKey::new("c", "Lb;")].names[1].as_deref(),
            Some("field_1")
        );
        assert_eq!(
            a.methods[&MemberKey::new("d", "([[Lb;I)La;")].names[1].as_deref(),
            Some("find")
        );
    }

    #[test]
    fn validate_finds_duplicates_and_bad_names() {
        let mut s = sample();
        s.set_class_name("b", 1, "pkg/Client").unwrap_err();
        s.classes.get_mut("b").unwrap().names[1] = Some("pkg/Client".into());
        s.set_member_name("a", Kind::Method, ("e", "([[Lb;I)La;"), 1, "find")
            .unwrap();
        s.set_member_name("a", Kind::Field, ("x", "Lzz;"), 1, "bad.name")
            .unwrap();
        let errors = s.validate();
        assert!(
            errors
                .iter()
                .any(|e| e.contains("duplicate class pkg/Client")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("duplicate Method")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("invalid member name")),
            "{errors:?}"
        );
        assert!(
            errors.iter().any(|e| e.contains("unmapped class zz")),
            "{errors:?}"
        );
    }

    #[test]
    fn deterministic_iteration() {
        let mut s = MappingSet::new(&["official"]);
        for k in ["b", "a", "c"] {
            s.class_mut(k);
        }
        assert_eq!(s.classes.keys().collect::<Vec<_>>(), ["a", "b", "c"]);
    }
}
