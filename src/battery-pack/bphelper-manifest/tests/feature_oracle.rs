//! Oracle harness: asserts our recommender names the same set of direct dependencies as cargo's own resolver for every fixture x feature-combo in `CASES`.
//! 
//! Gated behind the `oracle` cargo feature because each test shells out to `cargo metadata` (slow, registry-bound). Run with:
//! 
//! ```text
//! cargo test -p bphelper-manifest --features oracle
//! ```
//! 
//! See `md/spec/feature-refs.md` § Oracle agreement.

#![cfg(feature = "oracle")]
use std::{
    collections::{BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::LazyLock,
};

use bphelper_manifest::parse_battery_pack_from_path;
use cargo_metadata::{CargoOpt, DependencyKind, Metadata, MetadataCommand};
use indoc::formatdoc;

/// Set of direct-dependency names activated under a feature combo.
type DepSet = BTreeSet<String>;

/// One fixture x feature-combo expectation: our recommender's deps must equal cargo's resolved deps for `combo` against `pack`.
struct Case {
    pack: &'static str,
    combo: &'static [&'static str],
}

/// Fixtures x feature combos where our recommender's projection must match cargo's own resolution.
const CASES: &[Case] = &[
    Case {
        pack: "optional-feature-battery-pack",
        combo: &["fancy"],
    },
    Case {
        pack: "feature-syntax-battery-pack",
        combo: &[],
    },
    Case {
        pack: "feature-syntax-battery-pack",
        combo: &["derive"],
    },
    Case {
        pack: "feature-syntax-battery-pack",
        combo: &["weak-derive"],
    },
    Case {
        pack: "feature-syntax-battery-pack",
        combo: &["bundle"],
    },
    Case {
        pack: "mixed-kinds-battery-pack",
        combo: &[],
    },
    Case {
        pack: "mixed-kinds-battery-pack",
        combo: &["all"],
    },
];

/// Shared `test/fixtures` directory at the workspace root.
static FIXTURES_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("workspace root above bphelper-manifest")
        .join("tests/fixtures")
});

fn ours_deps(pack_dir: &Path, combo: &[&str]) -> DepSet {
    let spec = parse_battery_pack_from_path(&pack_dir.join("Cargo.toml"))
        .unwrap_or_else(|err| panic!("our parse failed for {}: {}", pack_dir.display(), err));

    spec.resolve_crates(combo).into_keys().collect()
}

/// Run `cargo metadata` against `pack_dir` activating `combo` (or `default` when empty).
fn run_metadata(pack_dir: &Path, combo: &[&str]) -> Metadata {
    let features = if combo.is_empty() {
        vec!["default".to_owned()]
    } else {
        combo.iter().copied().map(str::to_owned).collect()
    };

    MetadataCommand::new()
        .manifest_path(pack_dir.join("Cargo.toml"))
        .features(CargoOpt::SomeFeatures(features))
        .other_options(vec!["--no-default-features".to_owned()])
        .exec()
        .unwrap_or_else(|err| panic!("cargo metadata failed for {}: {}", pack_dir.display(), err))
}

/// Direct deps cargo activates for the combo.
///
/// `node.deps` lists every declared dep regardless of feature activation, 
/// so we filter the activated subset:
///   - non-optional Normal -> always activated
///   - dev / build deps -> always included (matches `dev-build-always`)
///   - optional Normal deps -> activated only if their package name appears in
///  `node.features` (cargo's implicit-feature signal)
fn cargo_deps(pack_dir: &Path, combo: &[&str]) -> DepSet {
    let metadata = run_metadata(pack_dir, combo);

    // locate the root package's node and manifest entry in the resolved graph
    let resolve = metadata.resolve.as_ref().expect("resolve graph preset");
    let root_id = resolve.root.as_ref().expect("root package present");

    let root_node = resolve
        .nodes
        .iter()
        .find(|nd| &nd.id == root_id)
        .expect("root node present in resolve graph");

    let root_pkg = metadata
        .packages
        .iter()
        .find(|pkg| &pkg.id == root_id)
        .expect("root package metadata present");

    // lookup tables for classifying each direct dep `node.deps[].name` is the extern (Rust identifier) name;
    // recover the canonical package name via the packages list for hyphen-preserving comparison.
    let pkg_name_by_id = metadata
        .packages
        .iter()
        .map(|pkg| (&pkg.id, pkg.name.as_str()))
        .collect::<HashMap<_, _>>();

    let optional_by_name = root_pkg
        .dependencies
        .iter()
        .filter(|dep| dep.optional)
        .map(|dep| dep.name.as_str())
        .collect::<BTreeSet<_>>();
    let active_features = root_node
        .features
        .iter()
        .map(AsRef::as_ref)
        .collect::<BTreeSet<_>>();

    // walk root edges, including each dep that satisfies any inclusion rule
    let mut activated = DepSet::new();
    for dep in &root_node.deps {
        let pkg_name = pkg_name_by_id[&dep.pkg];
        let is_dev_or_build = !dep
            .dep_kinds
            .iter()
            .any(|kd| matches!(kd.kind, DependencyKind::Normal));
        let is_optional = optional_by_name.contains(pkg_name);
        let cargo_activated = active_features.contains(pkg_name);

        if is_dev_or_build || !is_optional || cargo_activated {
            activated.insert(pkg_name.to_owned());
        }
    }

    activated
}

#[test]
fn oracle_agrees_for_every_case() {
    let mut failures = Vec::new();
    for case in CASES {
        let pack_dir = FIXTURES_DIR.join(case.pack);
        let ours = ours_deps(&pack_dir, case.combo);
        let cargo = cargo_deps(&pack_dir, case.combo);

        if ours != cargo {
            failures.push(formatdoc! {"
              pack: {pack}
              combo: {combo:?}
              ours: {ours:?}
              cargo: {cargo:?}
              ", 
                  pack = case.pack,
                  combo = case.combo,
                  ours = ours,
                  cargo = cargo
            });
        }
    }

    assert!(
        failures.is_empty(),
        "oracle disagreements: {}",
        failures.join("\n")
    );
}
