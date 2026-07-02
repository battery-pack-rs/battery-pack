## Feature References

This section specifies how `cargo bp` interprets the string inside `[features]` list when resolving which crates and Cargo features appear in the recommended downstream `Cargo.toml` .

## Reference forms

r[feature-refs.forms]
A feature reference is one of:


| Form          | Variant                                    | Example                     |
| ------------- | ------------------------------------------ | --------------------------- |
| `foo`         | `Feature("foo")`                           | `default = ["foo"]`         |
| `dep:foo`     | `Dep("foo")`                               | `default = ["dep:foo"]`     |
| `foo/bar`     | `DepFeature { dep, feature, weak: false }` | `fancy = ["serde/derive"]`  |
| `foo?/bar`    | `DepFeature { dep, feature, weak: true }`  | `fancy = ["serde?/derive"]` |
| `pkg:foo/bar` | Namespaced (reserved)                      | `default = ["pkg:foo/bar"]` |


r[feature-refs.parse] References are parsed at manifest load time. A parse failure surfaces as a typed error against the originating `(feature_name, ref_string)` pair.

r[feature-refs.weak-equivalence]
Strong `foo/bar` and weak `foo?/bar` both add `bar` to the recommended features of `foo` when `foo` is activated. They differ
only in dep activation: strong activates `foo`; weak does not.
See [feature-refs.resolution.weak].

r[feature-refs.namespaced]
Namespaced references (`pkg:foo/bar`, per RFC 3143) parse successfully but are skipped by `resolve_crates` and emit a warning.

## Resolution

r[feature-refs.resolution.feature]
For `Feature(name)`, the recommender first checks whether `name` matches a key in `[features]`. If so, that feature's reference list
is expanded inline per [feature-refs.resolution.recursion].
Otherwise `name` is treated as a crate in `[dependencies]` and added to the result with the features declared on the `[dependencies]` row.

r[feature-refs.resolution.dep]
For `Dep(name)`, the reference always refers to a crate in `[dependencies]`. Resolution otherwise matches the dep-name branch
of [feature-refs.resolution.feature].

r[feature-refs.resolution.dep-feature]
For strong `DepFeature { dep, feature, weak: false }`, the recommender adds `dep` to the result (if not already present) with
its declared row features, then unions `feature` into the result's feature set.

r[feature-refs.resolution.weak]
For weak `DepFeature { dep, feature, weak: true }`, the recommender records the `(dep, feature)` pair as a deferred entry. After all non-weak references in the combo have been resolved, each deferred entry is applied only if `dep` is already in the result map (activated by another reference). Otherwise the entry is dropped and `dep` is not added.

r[feature-refs.resolution.recursion]
A `Feature(name)` whose `name` matches a key in `[features]` is expanded by recursively resolving the referenced feature's own reference list. Crates added by inner expansion are merged into the result as if directly referenced. Cycles are rejected at validation time per [feature-refs.validation.cycles].

r[feature-refs.resolution.dev-build]
Crates with `dep_kind` other than `Normal` (dev, build) are always included in the result regardless of which features are active, matching [format.features.dev-build-always].

## Validation

r[feature-refs.validation.unknown]
A reference whose `dep` part matches neither a declared dependency nor a local feature name is a validation error.

r[feature-refs.validation.cycles] A cycle through local feature references is a validation error. Example: `a = ["b"]`, `b = ["a"]`.

## Oracle agreement

r[feature-refs.oracle]
For any `(pack, feature-combo)` pair the recommender MUST name the same set of direct dependencies as `cargo metadata --features <combo>` for the same pack. An oracle test harness, gated behind the `oracle` cargo feature and run in CI, enforces this invariant.

r[feature-refs.oracle.scope] The oracle compares dep-membership only, not the activated feature set per dep. Cargo's resolver activates each dep's own transitive default features (e.g. `serde`'s `std`, `alloc`); the recommender emits only what the pack's `[features]` ask for. Per-dep feature correctness is verified by in-process unit tests against the recommender's own output.

r[feature-refs.cargo-upgrades]
A failing oracle test after a `cargo` upgrade is resolved by updating the recommender to match cargo's new behaviour.