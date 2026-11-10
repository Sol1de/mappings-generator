//! JVM descriptors: class references inside field and method descriptors.

/// Rewrites every class reference of a field or method descriptor with `map`.
/// Names `map` returns `None` for (JDK, libraries) are kept unchanged.
pub fn remap_descriptor<'a>(desc: &str, map: impl Fn(&str) -> Option<&'a str>) -> String {
    let mut out = String::with_capacity(desc.len());
    let mut rest = desc;
    while let Some(start) = rest.find('L') {
        out.push_str(&rest[..=start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(';') else {
            out.push_str(after);
            return out;
        };
        let name = &after[..end];
        out.push_str(map(name).unwrap_or(name));
        out.push(';');
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Class names referenced by a descriptor, in order.
pub fn descriptor_classes(desc: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = desc;
    // Scan left to right so an `L` inside a class name is never taken as a type tag.
    while let Some(start) = rest.find('L') {
        let Some(end) = rest[start..].find(';') else {
            break;
        };
        out.push(&rest[start + 1..start + end]);
        rest = &rest[start + end + 1..];
    }
    out
}

/// Checks a field (`method = false`) or method descriptor (JVMS 4.3).
pub fn valid_descriptor(desc: &str, method: bool) -> bool {
    fn field_type(s: &[u8], mut i: usize) -> Option<usize> {
        let mut dims = 0;
        while s.get(i) == Some(&b'[') {
            dims += 1;
            i += 1;
        }
        if dims > 255 {
            return None;
        }
        match s.get(i)? {
            b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' => Some(i + 1),
            b'L' => {
                let end = i + 1 + s[i + 1..].iter().position(|&b| b == b';')?;
                let name = &s[i + 1..end];
                (!name.is_empty() && !name.contains(&b'.') && !name.contains(&b'['))
                    .then_some(end + 1)
            }
            _ => None,
        }
    }
    let s = desc.as_bytes();
    if !method {
        return field_type(s, 0) == Some(s.len());
    }
    if s.first() != Some(&b'(') {
        return false;
    }
    let mut i = 1;
    while s.get(i) != Some(&b')') {
        match field_type(s, i) {
            Some(next) => i = next,
            None => return false,
        }
    }
    i += 1;
    s.get(i..) == Some(b"V") || field_type(s, i) == Some(s.len())
}

/// Converts a Java source type (`int`, `java.lang.String[]`) to a descriptor
/// (dotted class names become internal names).
pub fn java_type_to_descriptor(ty: &str) -> Option<String> {
    let (mut base, mut dims) = (ty, 0);
    while let Some(rest) = base.strip_suffix("[]") {
        dims += 1;
        base = rest;
    }
    let base_desc = match base {
        "boolean" => "Z".to_owned(),
        "byte" => "B".to_owned(),
        "char" => "C".to_owned(),
        "short" => "S".to_owned(),
        "int" => "I".to_owned(),
        "long" => "J".to_owned(),
        "float" => "F".to_owned(),
        "double" => "D".to_owned(),
        "void" if dims == 0 => "V".to_owned(),
        "" | "void" => return None,
        name if name.contains(|c: char| c.is_whitespace() || c == '/' || c == ';') => return None,
        name => format!("L{};", name.replace('.', "/")),
    };
    Some(format!("{}{base_desc}", "[".repeat(dims)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaps_nested_arrays_and_keeps_external() {
        let map = |n: &str| (n == "a").then_some("net/minecraft/A");
        assert_eq!(
            remap_descriptor("([[La;ILjava/lang/String;)[La;", map),
            "([[Lnet/minecraft/A;ILjava/lang/String;)[Lnet/minecraft/A;"
        );
        assert_eq!(remap_descriptor("I", map), "I");
    }

    #[test]
    fn validates_descriptors() {
        assert!(valid_descriptor("()V", true));
        assert!(valid_descriptor("([[ILa/b;J)La;", true));
        assert!(valid_descriptor("[[Ljava/lang/String;", false));
        assert!(!valid_descriptor("V", false));
        assert!(!valid_descriptor("(V)V", true));
        assert!(!valid_descriptor("(La.b;)V", true));
        assert!(!valid_descriptor("(I", true));
        assert!(!valid_descriptor("Lx", false));
    }

    #[test]
    fn java_types() {
        assert_eq!(java_type_to_descriptor("int").unwrap(), "I");
        assert_eq!(
            java_type_to_descriptor("java.lang.String[][]").unwrap(),
            "[[Ljava/lang/String;"
        );
        assert_eq!(java_type_to_descriptor("void").unwrap(), "V");
        assert!(java_type_to_descriptor("void[]").is_none());
        assert_eq!(
            descriptor_classes("(La;[Lb/c;ILjava/util/List;)V"),
            ["a", "b/c", "java/util/List"]
        );
    }
}
