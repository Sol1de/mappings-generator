//! Runtime namespace detection from the classes actually loaded
//! (spec/namespaces.md#detection-lunar-and-verification).

use crate::model::{JarClass, Kind, MappingSet};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};

pub const THRESHOLD: f64 = 0.98;
pub const MARGIN: f64 = 0.10;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Verdict {
    pub method: &'static str,
    pub classes: String,
    pub members: String,
    pub class_scores: BTreeMap<String, f64>,
    pub member_scores: BTreeMap<String, f64>,
    pub unmatched_classes: usize,
    pub unmatched_members: usize,
}

fn round(x: f64) -> f64 {
    (x * 10_000.0).round() / 10_000.0
}

/// Best candidate if it reaches the threshold and beats the runner-up by the margin.
fn select(scores: &BTreeMap<String, (usize, usize)>) -> Option<String> {
    let mut ranked: Vec<(&String, f64)> = scores
        .iter()
        .map(|(k, &(hit, total))| {
            (
                k,
                if total == 0 {
                    0.0
                } else {
                    hit as f64 / total as f64
                },
            )
        })
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    match ranked.as_slice() {
        [(best, s), rest @ ..]
            if *s >= THRESHOLD && rest.first().is_none_or(|r| s - r.1 >= MARGIN) =>
        {
            Some((*best).clone())
        }
        _ => None,
    }
}

/// Unique structural matches between `official` classes and runtime classes, by fingerprint.
pub fn structural(
    official: &BTreeMap<String, [u8; 32]>,
    runtime: &BTreeMap<String, [u8; 32]>,
) -> (BTreeMap<String, String>, usize) {
    let index = |m: &BTreeMap<String, [u8; 32]>| {
        let mut out: HashMap<[u8; 32], Vec<String>> = HashMap::new();
        for (k, f) in m {
            out.entry(*f).or_default().push(k.clone());
        }
        out
    };
    let (theirs, ours) = (index(runtime), index(official));
    let mut matches = BTreeMap::new();
    for (f, names) in &ours {
        if let ([a], Some([b])) = (names.as_slice(), theirs.get(f).map(Vec::as_slice)) {
            matches.insert(a.clone(), b.clone());
        }
    }
    let unmatched = official.len() - matches.len();
    (matches, unmatched)
}

/// Detects which namespaces of `set` (keyed by `official`) the runtime jar uses for classes,
/// then for members of the matched classes.
pub fn detect(
    set: &MappingSet,
    runtime: &BTreeMap<String, JarClass>,
    official_fp: &BTreeMap<String, [u8; 32]>,
    runtime_fp: &BTreeMap<String, [u8; 32]>,
) -> Result<Verdict, String> {
    // Only symbols found at runtime under some candidate name are evidence: a runtime cache
    // may omit classes (Lunar bakes almost, not exactly, every class).
    let present: Vec<_> = set
        .classes
        .values()
        .filter(|c| c.names.iter().flatten().any(|n| runtime.contains_key(n)))
        .collect();
    let mut class_scores = BTreeMap::new();
    for (i, ns) in set.namespaces.iter().enumerate() {
        let names: Vec<&str> = present
            .iter()
            .filter_map(|c| c.names[i].as_deref())
            .collect();
        let hit = names.iter().filter(|n| runtime.contains_key(**n)).count();
        class_scores.insert(ns.clone(), (hit, names.len()));
    }
    let as_ratio = |m: &BTreeMap<String, (usize, usize)>| -> BTreeMap<String, f64> {
        m.iter()
            .map(|(k, &(h, t))| {
                (
                    k.clone(),
                    round(if t == 0 { 0.0 } else { h as f64 / t as f64 }),
                )
            })
            .collect()
    };
    let Some(class_ns) = select(&class_scores) else {
        let (matches, unmatched) = structural(official_fp, runtime_fp);
        return Err(format!(
            "ambiguous runtime class namespace, scores {:?}; structural fallback: {} unique matches, {unmatched} unmatched",
            as_ratio(&class_scores),
            matches.len()
        ));
    };
    let ci = set.ns(&class_ns).expect("scored namespace");
    let map = set.class_map(ci);
    let mut member_scores: BTreeMap<String, (usize, usize)> =
        set.namespaces.iter().map(|n| (n.clone(), (0, 0))).collect();
    for c in set.classes.values() {
        let Some(rt) = c.names[ci].as_deref().and_then(|n| runtime.get(n)) else {
            continue;
        };
        for kind in [Kind::Field, Kind::Method] {
            let list = match kind {
                Kind::Field => &rt.fields,
                Kind::Method => &rt.methods,
            };
            for (k, m) in c.members(kind) {
                let desc = MappingSet::desc_in(&map, &k.desc);
                let found = |name: &str| list.iter().any(|(n, d, _)| n == name && *d == desc);
                if !m.names.iter().flatten().any(|n| found(n)) {
                    continue;
                }
                for (j, ns) in set.namespaces.iter().enumerate() {
                    let Some(name) = m.names[j].as_deref().filter(|n| !n.starts_with('<')) else {
                        continue;
                    };
                    let score = member_scores.get_mut(ns).expect("all namespaces");
                    score.1 += 1;
                    score.0 += usize::from(found(name));
                }
            }
        }
    }
    let Some(member_ns) = select(&member_scores) else {
        return Err(format!(
            "ambiguous runtime member namespace, scores {:?}",
            as_ratio(&member_scores)
        ));
    };
    let unmatched_classes = set.classes.len() - class_scores[&class_ns].0;
    let unmatched_members = member_scores[&member_ns].1 - member_scores[&member_ns].0;
    Ok(Verdict {
        method: "detected",
        classes: class_ns,
        members: member_ns,
        class_scores: as_ratio(&class_scores),
        member_scores: as_ratio(&member_scores),
        unmatched_classes,
        unmatched_members,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set() -> MappingSet {
        let mut s = MappingSet::new(&["official", "intermediary", "mojang"]);
        for i in 0..100 {
            let o = format!("c{i}");
            s.set_class_name(&o, 1, &format!("net/minecraft/class_{i}"))
                .unwrap();
            s.set_class_name(&o, 2, &format!("net/minecraft/Named{i}"))
                .unwrap();
            s.set_member_name(&o, Kind::Method, ("a", "()V"), 1, &format!("method_{i}"))
                .unwrap();
            s.set_member_name(&o, Kind::Method, ("a", "()V"), 2, &format!("tick{i}"))
                .unwrap();
        }
        s
    }

    fn runtime(
        class: impl Fn(usize) -> String,
        method: impl Fn(usize) -> String,
    ) -> BTreeMap<String, JarClass> {
        (0..100)
            .map(|i| {
                (
                    class(i),
                    JarClass {
                        methods: vec![(method(i), "()V".into(), 1)],
                        ..JarClass::default()
                    },
                )
            })
            .collect()
    }

    #[test]
    fn detects_intermediary_and_mixed_namespaces() {
        let s = set();
        let rt = runtime(
            |i| format!("net/minecraft/class_{i}"),
            |i| format!("method_{i}"),
        );
        let v = detect(&s, &rt, &BTreeMap::new(), &BTreeMap::new()).unwrap();
        assert_eq!(
            (v.classes.as_str(), v.members.as_str()),
            ("intermediary", "intermediary")
        );
        let mixed = runtime(
            |i| format!("net/minecraft/Named{i}"),
            |i| format!("method_{i}"),
        );
        let v = detect(&s, &mixed, &BTreeMap::new(), &BTreeMap::new()).unwrap();
        assert_eq!(
            (v.classes.as_str(), v.members.as_str()),
            ("mojang", "intermediary")
        );
    }

    #[test]
    fn renamed_jar_is_ambiguous_with_scores() {
        let s = set();
        let renamed = runtime(|i| format!("x/Renamed{i}"), |i| format!("m{i}"));
        let err = detect(&s, &renamed, &BTreeMap::new(), &BTreeMap::new()).unwrap_err();
        assert!(
            err.contains("ambiguous") && err.contains("intermediary"),
            "{err}"
        );
        let half = runtime(
            |i| {
                if i % 2 == 0 {
                    format!("net/minecraft/class_{i}")
                } else {
                    format!("net/minecraft/Named{i}")
                }
            },
            |i| format!("method_{i}"),
        );
        assert!(detect(&s, &half, &BTreeMap::new(), &BTreeMap::new()).is_err());
    }

    #[test]
    fn structural_accepts_unique_matches_only() {
        let fp = |b: u8| [b; 32];
        let official = BTreeMap::from([
            ("a".to_string(), fp(1)),
            ("b".to_string(), fp(2)),
            ("c".to_string(), fp(2)),
        ]);
        let runtime = BTreeMap::from([("x/A".to_string(), fp(1)), ("x/B".to_string(), fp(2))]);
        let (m, unmatched) = structural(&official, &runtime);
        assert_eq!(m, BTreeMap::from([("a".to_string(), "x/A".to_string())]));
        assert_eq!(unmatched, 2);
    }
}
