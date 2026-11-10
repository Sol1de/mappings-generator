//! Downstream conversion of `mappings` outputs into the mc-mappings table format
//! (schema version 1, see `mc_mappings/spec/table-format.md`).
//!
//! The input catalog names each semantic symbol by its Mojang name and its semantic types;
//! the runtime names, declaring classes, static flags and descriptors come from the generated
//! files. Every catalog symbol must resolve in every configuration, or the export fails.

use super::json::{ClassRec, File};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

pub const CATALOG_FORMAT: &str = "mc_mappings_catalog";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Type {
    Primitive { name: String },
    Class { id: String },
    External { runtime_name: String },
    Array { element: Box<Type> },
    Void,
}

/// The Mojang name for `version`: `mojang`, unless `mojang_by_version` overrides it
/// (renamed symbols).
fn for_version<'a>(
    mojang: &'a str,
    by_version: &'a BTreeMap<String, String>,
    version: &str,
) -> &'a str {
    by_version.get(version).map_or(mojang, String::as_str)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogClass {
    pub id: String,
    pub mojang: String,
    #[serde(default)]
    pub mojang_by_version: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogField {
    pub id: String,
    pub owner: String,
    pub mojang: String,
    #[serde(default)]
    pub mojang_by_version: BTreeMap<String, String>,
    #[serde(rename = "type")]
    pub field_type: Type,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogMethod {
    pub id: String,
    pub owner: String,
    pub mojang: String,
    #[serde(default)]
    pub mojang_by_version: BTreeMap<String, String>,
    pub parameters: Vec<Type>,
    pub returns: Type,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalog {
    pub format: String,
    pub format_version: u32,
    #[serde(default)]
    #[allow(dead_code)] // free text for humans
    pub description: String,
    pub classes: Vec<CatalogClass>,
    pub fields: Vec<CatalogField>,
    pub methods: Vec<CatalogMethod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ClassRecord {
    pub id: String,
    pub runtime_name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FieldRecord {
    pub id: String,
    pub owner: String,
    pub runtime_name: String,
    #[serde(rename = "static")]
    pub is_static: bool,
    #[serde(rename = "type")]
    pub field_type: Type,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MethodRecord {
    pub id: String,
    pub owner: String,
    pub runtime_name: String,
    #[serde(rename = "static")]
    pub is_static: bool,
    pub parameters: Vec<Type>,
    pub returns: Type,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub profile_id: String,
    pub classes: Vec<ClassRecord>,
    pub fields: Vec<FieldRecord>,
    pub methods: Vec<MethodRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct ConfigurationRecord {
    pub minecraft_version: String,
    pub launcher: String,
    pub loader: String,
    pub profile_id: String,
}

#[derive(Debug, Serialize)]
pub struct CatalogIds {
    pub classes: Vec<String>,
    pub fields: Vec<String>,
    pub methods: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Dataset {
    pub schema_version: u32,
    pub data_version: u64,
    pub catalog: CatalogIds,
    pub configurations: Vec<ConfigurationRecord>,
    pub profiles: Vec<Profile>,
}

fn valid_id(id: &str) -> bool {
    id.split('.').all(|seg| {
        let mut b = seg.bytes();
        b.next().is_some_and(|c| c.is_ascii_lowercase())
            && b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
    })
}

impl Catalog {
    /// Checks identifiers (grammar, global uniqueness), owners and type references.
    pub fn validate(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if self.format != CATALOG_FORMAT || self.format_version != 1 {
            errors.push(format!(
                "unsupported catalog format {} v{}",
                self.format, self.format_version
            ));
        }
        let mut seen = std::collections::HashSet::new();
        let ids = self
            .classes
            .iter()
            .map(|c| &c.id)
            .chain(self.fields.iter().map(|f| &f.id))
            .chain(self.methods.iter().map(|m| &m.id));
        for id in ids {
            if !valid_id(id) {
                errors.push(format!("invalid semantic identifier {id:?}"));
            }
            if !seen.insert(id.as_str()) {
                errors.push(format!("duplicate identifier {id}"));
            }
        }
        let classes: std::collections::HashSet<&str> =
            self.classes.iter().map(|c| c.id.as_str()).collect();
        fn check_type(
            t: &Type,
            classes: &std::collections::HashSet<&str>,
            void_ok: bool,
            ctx: &str,
            errors: &mut Vec<String>,
        ) {
            match t {
                Type::Primitive { name }
                    if !matches!(
                        name.as_str(),
                        "boolean" | "byte" | "char" | "short" | "int" | "long" | "float" | "double"
                    ) =>
                {
                    errors.push(format!("{ctx}: unknown primitive {name}"))
                }
                Type::Class { id } if !classes.contains(id.as_str()) => {
                    errors.push(format!("{ctx}: unknown class id {id}"))
                }
                Type::External { runtime_name }
                    if runtime_name.contains('.') || runtime_name.is_empty() =>
                {
                    errors.push(format!("{ctx}: invalid external name {runtime_name:?}"))
                }
                Type::Array { element } => check_type(element, classes, false, ctx, errors),
                Type::Void if !void_ok => {
                    errors.push(format!("{ctx}: void is only allowed as a return type"))
                }
                _ => {}
            }
        }
        for f in &self.fields {
            if !classes.contains(f.owner.as_str()) {
                errors.push(format!("{}: unknown owner {}", f.id, f.owner));
            }
            check_type(&f.field_type, &classes, false, &f.id, &mut errors);
        }
        for m in &self.methods {
            if !classes.contains(m.owner.as_str()) {
                errors.push(format!("{}: unknown owner {}", m.id, m.owner));
            }
            for p in &m.parameters {
                check_type(p, &classes, false, &m.id, &mut errors);
            }
            check_type(&m.returns, &classes, true, &m.id, &mut errors);
        }
        errors
    }
}

/// Keys of the `names` objects holding Mojang names for classes and members in `file`.
fn mojang_keys(file: &File) -> Result<(&'static str, &'static str), String> {
    let pick = |alias: &serde_json::Value| -> Result<&'static str, String> {
        match alias.as_str() {
            Some("mojang") => Ok("runtime"),
            _ if file.namespaces.iter().any(|n| n == "mojang") => Ok("mojang"),
            _ => Err("no Mojang names in this file".into()),
        }
    };
    let runtime = &file.aliases["runtime"];
    if runtime.is_string() {
        Ok((pick(runtime)?, pick(runtime)?))
    } else {
        Ok((pick(&runtime["classes"])?, pick(&runtime["members"])?))
    }
}

fn primitive_desc(name: &str) -> char {
    match name {
        "boolean" => 'Z',
        "byte" => 'B',
        "char" => 'C',
        "short" => 'S',
        "int" => 'I',
        "long" => 'J',
        "float" => 'F',
        "double" => 'D',
        _ => unreachable!("validated catalog"),
    }
}

fn runtime_desc(t: &Type, class_rt: &HashMap<&str, &str>, out: &mut String) {
    match t {
        Type::Primitive { name } => out.push(primitive_desc(name)),
        Type::Class { id } => {
            out.push('L');
            out.push_str(class_rt[id.as_str()]);
            out.push(';');
        }
        Type::External { runtime_name } => {
            out.push('L');
            out.push_str(runtime_name);
            out.push(';');
        }
        Type::Array { element } => {
            out.push('[');
            runtime_desc(element, class_rt, out);
        }
        Type::Void => out.push('V'),
    }
}

/// Mapping-file-specific mc-mappings identity of a configuration.
pub fn identity(cfg: &super::json::Configuration) -> Result<(String, String), String> {
    let launcher = match cfg.launcher.as_str() {
        "official" => "standard",
        "lunar" => "lunar",
        other => return Err(format!("launcher {other} has no mc-mappings equivalent")),
    };
    let loader = match (cfg.launcher.as_str(), cfg.loader.as_str()) {
        (_, "vanilla") | ("lunar", "lunar") => "none",
        (_, "fabric") => "fabric",
        ("official", "forge") => "forge",
        (_, other) => return Err(format!("loader {other} has no mc-mappings equivalent")),
    };
    Ok((launcher.into(), loader.into()))
}

/// Declaring class (owner or nearest ancestor), runtime name and static flag of a member,
/// looked up by Mojang name and runtime descriptor.
fn find<'f>(
    by_runtime: &HashMap<&'f str, &'f ClassRec>,
    mojang_key: &str,
    owner_rt: &'f str,
    mojang: &str,
    desc: &str,
    methods: bool,
) -> Option<(&'f str, &'f str, bool)> {
    let mut queue = VecDeque::from([owner_rt]);
    let mut seen = HashSet::from([owner_rt]);
    while let Some(rt) = queue.pop_front() {
        let class = by_runtime.get(rt)?;
        let list = if methods {
            &class.methods
        } else {
            &class.fields
        };
        let hit = list
            .iter()
            .find(|m| m.descriptor == desc && m.names.get(mojang_key).is_some_and(|n| n == mojang));
        if let Some(m) = hit {
            return Some((
                class.names["runtime"].as_str(),
                m.names["runtime"].as_str(),
                m.is_static,
            ));
        }
        for p in class.super_name.iter().chain(&class.interfaces) {
            if by_runtime.contains_key(p.as_str()) && seen.insert(p.as_str()) {
                queue.push_back(p.as_str());
            }
        }
    }
    None
}

/// Builds the profile of one configuration. Returns every unresolved symbol on failure.
pub fn profile(catalog: &Catalog, file: &File, profile_id: &str) -> Result<Profile, Vec<String>> {
    let version = file.configuration.minecraft_version.as_str();
    let (ck, mk) = mojang_keys(file).map_err(|e| vec![e])?;
    let by_mojang: HashMap<&str, &ClassRec> = file
        .classes
        .iter()
        .filter_map(|c| c.names.get(ck).map(|n| (n.as_str(), c)))
        .collect();
    let by_runtime = file.by_runtime();
    let mut errors = Vec::new();
    let mut class_rt: HashMap<&str, &str> = HashMap::new();
    let mut id_of_rt: HashMap<&str, &str> = HashMap::new();
    let mut classes = Vec::new();
    for c in &catalog.classes {
        let name = for_version(&c.mojang, &c.mojang_by_version, version);
        match by_mojang.get(name) {
            Some(rec) => {
                let rt = rec.names["runtime"].as_str();
                class_rt.insert(&c.id, rt);
                if let Some(other) = id_of_rt.insert(rt, &c.id) {
                    errors.push(format!(
                        "{} and {other} both map to runtime class {rt}",
                        c.id
                    ));
                }
                classes.push(ClassRecord {
                    id: c.id.clone(),
                    runtime_name: rt.to_owned(),
                });
            }
            None => errors.push(format!("class {} ({name}) not found", c.id)),
        }
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let types = catalog.fields.iter().map(|f| &f.field_type).chain(
        catalog
            .methods
            .iter()
            .flat_map(|m| m.parameters.iter().chain([&m.returns])),
    );
    for mut t in types {
        while let Type::Array { element } = t {
            t = element;
        }
        if let Type::External { runtime_name } = t
            && (by_runtime.contains_key(runtime_name.as_str())
                || by_mojang.contains_key(runtime_name.as_str()))
        {
            errors.push(format!(
                "external type {runtime_name} is a mapped class: use its class id"
            ));
        }
    }
    let mut seen_members = HashSet::new();
    let mut fields = Vec::new();
    for f in &catalog.fields {
        let mut desc = String::new();
        runtime_desc(&f.field_type, &class_rt, &mut desc);
        let name = for_version(&f.mojang, &f.mojang_by_version, version);
        match find(
            &by_runtime,
            mk,
            class_rt[f.owner.as_str()],
            name,
            &desc,
            false,
        ) {
            Some((decl, rt_name, is_static)) => match id_of_rt.get(decl) {
                Some(owner) if seen_members.insert(('f', decl, rt_name, desc.clone())) => fields
                    .push(FieldRecord {
                        id: f.id.clone(),
                        owner: (*owner).to_owned(),
                        runtime_name: rt_name.to_owned(),
                        is_static,
                        field_type: f.field_type.clone(),
                    }),
                Some(_) => errors.push(format!(
                    "field {}: another catalog field maps to {decl}.{rt_name}",
                    f.id
                )),
                None => errors.push(format!(
                    "field {}: declared in {decl}, which is not a catalog class",
                    f.id
                )),
            },
            None => errors.push(format!(
                "field {} ({name} {desc}) not found from {}",
                f.id, f.owner
            )),
        }
    }
    let mut methods = Vec::new();
    for m in &catalog.methods {
        let mut desc = String::from("(");
        for p in &m.parameters {
            runtime_desc(p, &class_rt, &mut desc);
        }
        desc.push(')');
        runtime_desc(&m.returns, &class_rt, &mut desc);
        let name = for_version(&m.mojang, &m.mojang_by_version, version);
        match find(
            &by_runtime,
            mk,
            class_rt[m.owner.as_str()],
            name,
            &desc,
            true,
        ) {
            Some((decl, rt_name, is_static)) => match id_of_rt.get(decl) {
                Some(owner) if seen_members.insert(('m', decl, rt_name, desc.clone())) => methods
                    .push(MethodRecord {
                        id: m.id.clone(),
                        owner: (*owner).to_owned(),
                        runtime_name: rt_name.to_owned(),
                        is_static,
                        parameters: m.parameters.clone(),
                        returns: m.returns.clone(),
                    }),
                Some(_) => errors.push(format!(
                    "method {}: another catalog method maps to {decl}.{rt_name}{desc}",
                    m.id
                )),
                None => errors.push(format!(
                    "method {}: declared in {decl}, which is not a catalog class",
                    m.id
                )),
            },
            None => errors.push(format!(
                "method {} ({name}{desc}) not found from {}",
                m.id, m.owner
            )),
        }
    }
    if errors.is_empty() {
        Ok(Profile {
            profile_id: profile_id.to_owned(),
            classes,
            fields,
            methods,
        })
    } else {
        Err(errors)
    }
}

/// Converts generated files (one per configuration) into one dataset. Configurations whose
/// tables are identical share one profile; their identities stay separate.
pub fn export(
    catalog: &Catalog,
    files: &[File],
    data_version: u64,
) -> Result<Dataset, Vec<String>> {
    let mut errors = catalog.validate();
    if data_version == 0 {
        errors.push("data_version must be positive".into());
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut entries = Vec::new();
    for file in files {
        match identity(&file.configuration) {
            Ok((launcher, loader)) => entries.push((
                file.configuration.minecraft_version.clone(),
                launcher,
                loader,
                file,
            )),
            Err(e) => errors.push(e),
        }
    }
    entries.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
    let mut profiles: Vec<Profile> = Vec::new();
    let mut configurations: Vec<ConfigurationRecord> = Vec::new();
    for (version, launcher, loader, file) in entries {
        let own_id = format!("mc{}.{launcher}.{loader}", version.replace(['.', '-'], "_"));
        if configurations.iter().any(|c| {
            (&c.minecraft_version, &c.launcher, &c.loader) == (&version, &launcher, &loader)
        }) {
            errors.push(format!(
                "duplicate configuration {version} {launcher} {loader}"
            ));
            continue;
        }
        let profile = match profile(catalog, file, &own_id) {
            Ok(p) => p,
            Err(es) => {
                errors.extend(
                    es.into_iter()
                        .map(|e| format!("{version} {launcher} {loader}: {e}")),
                );
                continue;
            }
        };
        let same = |p: &&Profile| {
            (&p.classes, &p.fields, &p.methods)
                == (&profile.classes, &profile.fields, &profile.methods)
        };
        let profile_id = match profiles.iter().find(same) {
            Some(existing) => existing.profile_id.clone(),
            None => {
                profiles.push(profile);
                own_id
            }
        };
        configurations.push(ConfigurationRecord {
            minecraft_version: version,
            launcher,
            loader,
            profile_id,
        });
    }
    if !errors.is_empty() {
        return Err(errors);
    }
    // Name each profile after every configuration that uses it, so a shared table never looks
    // specific to one launcher.
    for p in &mut profiles {
        let users: Vec<String> = configurations
            .iter()
            .filter(|c| c.profile_id == p.profile_id)
            .map(|c| {
                format!(
                    "mc{}_{}_{}",
                    c.minecraft_version.replace(['.', '-'], "_"),
                    c.launcher,
                    c.loader
                )
            })
            .collect();
        let new_id = users.join(".");
        for c in configurations
            .iter_mut()
            .filter(|c| c.profile_id == p.profile_id)
        {
            c.profile_id.clone_from(&new_id);
        }
        p.profile_id = new_id;
    }
    profiles.sort_by(|a, b| a.profile_id.cmp(&b.profile_id));
    Ok(Dataset {
        schema_version: 1,
        data_version,
        catalog: CatalogIds {
            classes: catalog.classes.iter().map(|c| c.id.clone()).collect(),
            fields: catalog.fields.iter().map(|f| f.id.clone()).collect(),
            methods: catalog.methods.iter().map(|m| m.id.clone()).collect(),
        },
        configurations,
        profiles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::json;

    /// A two-class file; Mojang names are either extra keys or the runtime names themselves.
    fn file(launcher: &str, loader: &str, version: &str, runtime_is_mojang: bool) -> File {
        let (client, entity, player, tick) = (
            "net/minecraft/client/Minecraft",
            "net/minecraft/Entity",
            "player",
            "tick",
        );
        let rt =
            |obf: &'static str, mojang: &'static str| if runtime_is_mojang { mojang } else { obf };
        let names = |obf: &'static str, mojang: &'static str| {
            if runtime_is_mojang {
                serde_json::json!({ "runtime": mojang })
            } else {
                serde_json::json!({ "runtime": obf, "mojang": mojang })
            }
        };
        let value = serde_json::json!({
            "format": "mappings", "format_version": 1,
            "configuration": { "minecraft_version": version, "launcher": launcher, "loader": loader },
            "namespaces": if runtime_is_mojang { serde_json::json!(["runtime"]) } else { serde_json::json!(["runtime", "mojang"]) },
            "aliases": { "runtime": if runtime_is_mojang { "mojang" } else { "official" } },
            "classes": [
                { "names": names("a", client), "super": rt("b", entity), "interfaces": [],
                  "fields": [{ "names": names("c", player), "descriptor": format!("L{};", rt("b", entity)), "access": 1, "static": false }],
                  "methods": [{ "names": names("<init>", "<init>"), "descriptor": "()V", "access": 1, "static": false }] },
                { "names": names("b", entity), "interfaces": [], "fields": [],
                  "methods": [{ "names": names("d", tick), "descriptor": format!("([[ILjava/lang/String;)L{};", rt("b", entity)), "access": 9, "static": true }] }
            ],
            "provenance": {}
        });
        json::read(value.to_string().as_bytes()).unwrap()
    }

    fn catalog() -> Catalog {
        serde_json::from_value(serde_json::json!({
            "format": "mc_mappings_catalog", "format_version": 1,
            "classes": [
                { "id": "minecraft.client", "mojang": "net/minecraft/client/Minecraft" },
                { "id": "minecraft.entity", "mojang": "net/minecraft/Entity" }
            ],
            "fields": [{ "id": "minecraft.client.player", "owner": "minecraft.client", "mojang": "player", "type": { "kind": "class", "id": "minecraft.entity" } }],
            "methods": [
                { "id": "minecraft.client.new", "owner": "minecraft.client", "mojang": "<init>", "parameters": [], "returns": { "kind": "void" } },
                { "id": "minecraft.client.tick", "owner": "minecraft.client", "mojang": "tick",
                  "parameters": [
                      { "kind": "array", "element": { "kind": "array", "element": { "kind": "primitive", "name": "int" } } },
                      { "kind": "external", "runtime_name": "java/lang/String" }
                  ],
                  "returns": { "kind": "class", "id": "minecraft.entity" } }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn exports_and_shares_identical_profiles() {
        let files = [
            file("official", "vanilla", "1.21", false),
            file("lunar", "lunar", "1.21", false),
            file("official", "forge", "1.21", true),
        ];
        let d = export(&catalog(), &files, 3).unwrap();
        assert_eq!((d.profiles.len(), d.configurations.len()), (2, 3));
        let lunar = d
            .configurations
            .iter()
            .find(|c| c.launcher == "lunar")
            .unwrap();
        assert_eq!(
            (lunar.loader.as_str(), lunar.profile_id.as_str()),
            ("none", "mc1_21_lunar_none.mc1_21_standard_none")
        );
        let vanilla = d
            .configurations
            .iter()
            .find(|c| c.loader == "none" && c.launcher == "standard")
            .unwrap();
        assert_eq!(
            vanilla.profile_id, "mc1_21_lunar_none.mc1_21_standard_none",
            "identical tables are shared"
        );
        let p = d
            .profiles
            .iter()
            .find(|p| p.profile_id == "mc1_21_lunar_none.mc1_21_standard_none")
            .unwrap();
        let tick = p
            .methods
            .iter()
            .find(|m| m.id == "minecraft.client.tick")
            .unwrap();
        assert_eq!(
            (
                tick.owner.as_str(),
                tick.runtime_name.as_str(),
                tick.is_static
            ),
            ("minecraft.entity", "d", true)
        );
        assert_eq!(p.fields[0].runtime_name, "c");
        let forge = d
            .profiles
            .iter()
            .find(|p| p.profile_id == "mc1_21_standard_forge")
            .unwrap();
        assert_eq!(
            forge.classes[0].runtime_name,
            "net/minecraft/client/Minecraft"
        );
        let text = serde_json::to_string(&d).unwrap();
        assert!(text.contains(r#"{"kind":"array","element":{"kind":"array","element":{"kind":"primitive","name":"int"}}}"#), "{text}");
        assert!(text.starts_with(r#"{"schema_version":1,"data_version":3,"catalog":{"classes":["minecraft.client","minecraft.entity"]"#));
    }

    #[test]
    fn missing_symbols_and_bad_catalogs_fail() {
        let vanilla = || [file("official", "vanilla", "1.21", false)];
        let mut c = catalog();
        c.methods[1].mojang = "missing".into();
        let err = export(&c, &vanilla(), 1).unwrap_err();
        assert!(
            err.iter()
                .any(|e| e.contains("minecraft.client.tick") && e.contains("not found")),
            "{err:?}"
        );
        let mut c = catalog();
        c.fields[0].field_type = Type::External {
            runtime_name: "b".into(),
        };
        let err = export(&c, &vanilla(), 1).unwrap_err();
        assert!(err.iter().any(|e| e.contains("external type b")), "{err:?}");
        let mut c = catalog();
        c.classes[0].id = "Minecraft.Client".into();
        assert!(export(&c, &[], 1).is_err());
        assert!(export(&catalog(), &[], 0).is_err());
        assert!(export(&catalog(), &[file("official", "quilt", "1.21", false)], 1).is_err());
    }

    #[test]
    fn per_version_names() {
        let mut c = catalog();
        c.classes[1].mojang = "net/minecraft/OldEntity".into();
        c.classes[1]
            .mojang_by_version
            .insert("1.21".into(), "net/minecraft/Entity".into());
        assert!(export(&c, &[file("official", "vanilla", "1.21", false)], 1).is_ok());
    }
}
