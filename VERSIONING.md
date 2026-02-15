# Versioning Policy (C5)

Semantic versioning applies to the **SDK contract**.

## BREAKING changes (MAJOR bump)
Any of the following in contract crates:

### Graph/Ports
- Renaming a `NodeKind`
- Removing a `NodeKind`
- Renaming any port name (`in`, `out`, `a`, `b`, `in0..in3`, etc.)
- Changing the default port set for an existing node kind

### Runtime semantics
- Changing meaning of an existing node kind
- Changing validation/invariant rules so graphs that previously validated now fail
- Changing determinism guarantees (compile plan ordering, scene/bank apply ordering)

### JSON / file formats
- Renaming JSON keys or changing required structure
- Removing accepted preset names
- Changing parsing so previously valid JSON fails

### Public API
- Removing a public type/function
- Changing a public signature in a contract crate

## NON-BREAKING changes (MINOR bump)
- Adding a new `NodeKind` (additive, does not change existing meanings)
- Adding new optional crates (outputs/plugins/backends) that depend on contract crates
- Adding new functions/types that do not alter existing behavior
- Adding feature flags that only add capability (no semantic changes)

## PATCH changes
- Bug fixes that do not alter contracts
- Documentation fixes
- Test/CI improvements
- Performance changes that do not change observable contract behavior

## Feature flags
Features may only add capability. No feature may change the meaning of existing graph nodes or runtime invariants.
