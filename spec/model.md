# Mapping model

## Structure

A `MappingSet` holds an ordered list of namespaces and a tree of classes.

```text
MappingSet
├─ namespaces: [ns0, ns1, ...]          # ns0 = descriptor namespace
└─ classes (keyed by ns0 name)
   ├─ names[ns] -> Option<String>       # internal names, slash-separated
   ├─ access: Option<u16>               # from the jar, if known
   ├─ super_name, interfaces            # ns0 names, from the jar
   ├─ fields (keyed by ns0 name + ns0 descriptor)
   │  └─ names[ns], access
   └─ methods (keyed by ns0 name + ns0 descriptor)
      └─ names[ns], access
```

- Descriptors are stored once, in `ns0`; descriptors in other namespaces are derived by remapping
  class names. They are never stored twice.
- `None` means "no name known in that namespace"; it is never filled with another namespace's name
  except by the explicit `complete` operation below.
- `static` is `access & ACC_STATIC`. Members without jar data have `access = None`.
- Ordered collections (`BTreeMap`) guarantee deterministic iteration.

## Operations

| Operation | Contract |
| --- | --- |
| `merge(a, b)` | Join two sets sharing a namespace; conflicting non-empty names are an error listing all conflicts |
| `compose(a: X→Y, b: Y→Z)` | Produce `X→Z`; entries of `a` whose `Y` name is absent from `b` are reported, not dropped silently |
| `invert / reorder(ns)` | Change `ns0`; descriptors are remapped accordingly |
| `rename_ns`, `drop_ns`, `alias(runtime, named)` | Namespace management |
| `complete(ns, policy)` | Fill missing names for unobfuscated symbols (lambdas, synthetic, `<init>`, Java overrides) with the `ns0` name, only where the jar proves the symbol is not obfuscated |
| `propagate()` | Copy method names along the inheritance graph (overrides of mapped methods), using jar hierarchy |
| `attach_jar(inventory)` | Add classes/members present in the jar but absent from mappings; set `access`, hierarchy |
| `remap_descriptor(desc, from, to)` | Field and method descriptors, arrays included |

## Invariants (checked by `validate`)

- Class names are unique per namespace.
- `(owner, name, descriptor)` is unique per namespace for fields and methods.
- Class internal names: non-empty `/`-separated segments, no `.`, `;`, `[`.
- Member names: non-empty, no `.`, `;`, `[`, `/`; `<` `>` only in `<init>`/`<clinit>`.
- Every class referenced by a descriptor is either mapped in the set or external (JDK/library),
  and external classes keep the same name in every namespace.
