use indoc::indoc;
use snapbox::{ToDebug, assert_data_eq, str};

use super::*;

// -- Config parsing --
// [verify format.templates.engine]

#[test]
fn parse_config_full() {
    let toml = r#"
            ignore = ["hooks", ".git"]

            [placeholders.description]
            type = "string"
            prompt = "Describe it"
            default = "A thing"

            [[files]]
            src = "shared/LICENSE"
            dest = "LICENSE"
        "#;
    let config: BpTemplateConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.ignore, vec!["hooks", ".git"]);
    assert_eq!(config.placeholders.len(), 1);
    let desc = &config.placeholders["description"];
    assert_eq!(desc.prompt.as_deref(), Some("Describe it"));
    assert_eq!(desc.default.as_deref(), Some("A thing"));
    assert_eq!(config.files.len(), 1);
    assert_eq!(config.files[0].src, "shared/LICENSE");
    assert_eq!(config.files[0].dest, "LICENSE");
}

#[test]
fn parse_config_empty() {
    let config: BpTemplateConfig = toml::from_str("").unwrap();
    assert!(config.ignore.is_empty());
    assert!(config.placeholders.is_empty());
    assert!(config.files.is_empty());
}

#[test]
fn parse_config_placeholder_defaults() {
    let toml = r#"
            [placeholders.name]
        "#;
    let config: BpTemplateConfig = toml::from_str(toml).unwrap();
    let p = &config.placeholders["name"];
    assert_data_eq!(
        p.to_debug(),
        str![[r#"
PlaceholderDef {
    prompt: None,
    default: None,
    placeholder_type: String,
}

"#]]
    );
    assert_eq!(p.placeholder_type, PlaceholderType::String);
}

// -- should_ignore --

#[test]
fn ignore_exact_match() {
    assert!(should_ignore(Path::new("hooks"), &["hooks"]));
}

#[test]
fn ignore_nested_component() {
    assert!(should_ignore(
        Path::new("hooks/pre-script.rhai"),
        &["hooks"]
    ));
}

#[test]
fn ignore_no_match() {
    assert!(!should_ignore(Path::new("src/main.rs"), &["hooks"]));
}

#[test]
fn ignore_bp_template_toml() {
    // bp-template.toml is excluded by a root-only check in the render pipeline,
    // NOT via should_ignore. Nested bp-template.toml files pass through.
    assert!(!should_ignore(Path::new("bp-template.toml"), &["hooks"]));
    assert!(!should_ignore(
        Path::new("templates/default/bp-template.toml"),
        &["hooks"]
    ));
}

// -- resolve_placeholders --
// [verify format.templates.placeholder-defaults]

#[test]
fn resolve_uses_define_over_default() {
    let mut defs = BTreeMap::new();
    defs.insert(
        "description".to_string(),
        PlaceholderDef {
            prompt: None,
            default: Some("fallback".to_string()),
            placeholder_type: PlaceholderType::String,
        },
    );
    let mut defines = BTreeMap::new();
    defines.insert("description".to_string(), "override".to_string());
    let mut vars = BTreeMap::new();

    resolve_placeholders(&defs, &defines, &mut vars, None).unwrap();
    assert_eq!(vars["description"], "override");
}

#[test]
fn resolve_uses_default_non_interactive() {
    let mut defs = BTreeMap::new();
    defs.insert(
        "description".to_string(),
        PlaceholderDef {
            prompt: None,
            default: Some("fallback".to_string()),
            placeholder_type: PlaceholderType::String,
        },
    );
    let defines = BTreeMap::new();
    let mut vars = BTreeMap::new();

    // In test/CI, stdout is not a terminal, so non-interactive path is taken
    resolve_placeholders(&defs, &defines, &mut vars, None).unwrap();
    assert_eq!(vars["description"], "fallback");
}

#[test]
fn resolve_no_default_non_interactive_errors() {
    let mut defs = BTreeMap::new();
    defs.insert(
        "description".to_string(),
        PlaceholderDef {
            prompt: Some("Describe it".to_string()),
            default: None,
            placeholder_type: PlaceholderType::String,
        },
    );
    let defines = BTreeMap::new();
    let mut vars = BTreeMap::new();

    let err = resolve_placeholders(&defs, &defines, &mut vars, None).unwrap_err();
    assert_data_eq!(
        err.to_string(),
        str!["placeholder 'description' has no default and no value provided"]
    );
}

#[test]
fn resolve_rejects_kebab_case_name() {
    let mut defs = BTreeMap::new();
    defs.insert(
        "my-thing".to_string(),
        PlaceholderDef {
            prompt: None,
            default: Some("val".to_string()),
            placeholder_type: PlaceholderType::String,
        },
    );
    let err =
        resolve_placeholders(&defs, &BTreeMap::new(), &mut BTreeMap::new(), None).unwrap_err();
    assert_data_eq!(
        err.to_string(),
        str!["placeholder 'my-thing' contains '-'; use snake_case (MiniJinja treats '-' as minus)"]
    );
}

// -- build_jinja_env --

#[test]
fn jinja_env_renders_variables() {
    let mut vars = BTreeMap::new();
    vars.insert("project_name".to_string(), "my-app".to_string());
    vars.insert("crate_name".to_string(), "my_app".to_string());

    let env = build_jinja_env(Path::new("."), &vars).unwrap();
    let result = env
        .render_str(
            "name = {{ project_name }}, crate = {{ crate_name }}",
            minijinja::context! {},
        )
        .unwrap();
    assert_eq!(result, "name = my-app, crate = my_app");
}

#[test]
fn jinja_env_no_html_escaping() {
    let mut vars = BTreeMap::new();
    vars.insert(
        "description".to_string(),
        "A <bold> & cool thing".to_string(),
    );

    let env = build_jinja_env(Path::new("."), &vars).unwrap();
    let result = env
        .render_str("{{ description }}", minijinja::context! {})
        .unwrap();
    assert_eq!(result, "A <bold> & cool thing");
}

#[test]
fn jinja_env_raw_block_passthrough() {
    let vars = BTreeMap::new();
    let env = build_jinja_env(Path::new("."), &vars).unwrap();
    let result = env
        .render_str(
            "{% raw %}{{ not_a_variable }}{% endraw %}",
            minijinja::context! {},
        )
        .unwrap();
    assert_eq!(result, "{{ not_a_variable }}");
}

// -- crate_name derivation --

#[test]
fn crate_name_derived_from_project_name() {
    let project_name = "my-cool-app";
    let crate_name = project_name.replace('-', "_");
    assert_eq!(crate_name, "my_cool_app");
}

#[test]
fn parse_config_unsupported_type_errors() {
    let toml = r#"
            [placeholders.flag]
            type = "bool"
        "#;
    let err = toml::from_str::<BpTemplateConfig>(toml).unwrap_err();
    assert_data_eq!(
        err.to_string(),
        str![[r#"
TOML parse error at line 3, column 20
  |
3 |             type = "bool"
  |                    ^^^^^^
unknown variant `bool`, expected `string`

"#]]
    );
}

// -- preview --

#[test]
fn preview_renders_template_in_memory() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/fancy-battery-pack");

    let opts = RenderOpts {
        crate_root: fixtures,
        template_path: "templates/default".to_string(),
        project_name: "my-project".to_string(),
        defines: BTreeMap::new(),
        interactive_override: None,
    };

    let files = preview(opts).unwrap();
    assert!(!files.is_empty(), "preview should produce files");

    // Should contain Cargo.toml with rendered project name
    let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();
    assert!(
        cargo.content.contains("my-project"),
        "Cargo.toml should contain rendered project name"
    );

    // Should contain src/main.rs
    assert!(
        files.iter().any(|f| f.path == "src/main.rs"),
        "should contain src/main.rs"
    );

    // Files should be sorted by path
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    let mut sorted = paths.clone();
    sorted.sort();
    assert_eq!(paths, sorted, "files should be sorted by path");
}

#[test]
fn preview_resolves_bp_managed_deps() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/managed-battery-pack");

    let opts = RenderOpts {
        crate_root: fixtures,
        template_path: "templates/default".to_string(),
        project_name: "my-project".to_string(),
        defines: BTreeMap::new(),
        interactive_override: None,
    };

    let files = preview(opts).unwrap();
    let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();

    assert!(
        cargo.content.contains(r#"name = "my-project""#),
        "Expected package name"
    );
    assert!(cargo.content.contains("anyhow"), "Expected anyhow");
    assert!(
        cargo.content.contains("managed-battery-pack"),
        "Expected managed dependency"
    );
}

#[test]
fn preview_warns_on_unresolvable_bp_managed_dep() {
    let tmp = tempfile::tempdir().unwrap();
    let crate_root = tmp.path();

    std::fs::write(
        crate_root.join("Cargo.toml"),
        indoc! {r#"
            [package]
            name = "fake-battery-pack"
            version = "0.1.0"
            edition = "2021"
            description = "test"
            keywords = ["battery-pack"]

            [package.metadata.battery.templates]
            default = { path = "templates/default", description = "test" }
        "#},
    )
    .unwrap();

    std::fs::create_dir_all(crate_root.join("src")).unwrap();
    std::fs::write(crate_root.join("src/lib.rs"), "").unwrap();

    let tpl_dir = crate_root.join("templates/default");
    std::fs::create_dir_all(&tpl_dir).unwrap();
    std::fs::write(
        tpl_dir.join("Cargo.toml"),
        indoc! {r#"
            [package]
            name = "{{ project_name }}"
            version = "0.1.0"
            edition = "2021"

            [dependencies]
            nonexistent-crate.bp-managed = true

            [package.metadata.battery-pack]
            fake-battery-pack = { features = ["default"] }
        "#},
    )
    .unwrap();

    let opts = RenderOpts {
        crate_root: crate_root.to_path_buf(),
        template_path: "templates/default".to_string(),
        project_name: "test-project".to_string(),
        defines: BTreeMap::new(),
        interactive_override: Some(false),
    };

    // Should succeed (warn, not error) since the battery pack may not exist yet
    let files = preview(opts).unwrap();
    let cargo = files.iter().find(|f| f.path == "Cargo.toml").unwrap();
    assert_data_eq!(
        &cargo.content,
        str![[r#"
[package]
name = "test-project"
version = "0.1.0"
edition = "2021"

[dependencies]
nonexistent-crate.bp-managed = true

[package.metadata.battery-pack]
fake-battery-pack = { features = ["default"] }
"#]]
    );
}

#[test]
fn preview_template_resolves_and_renders() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/fancy-battery-pack");

    let source = crate::registry::CrateSource::Local(fixtures.parent().unwrap().to_path_buf());
    let opts = PreviewOpts {
        battery_pack: "fancy",
        template: "default",
        path: None,
        source: &source,
    };

    let (crate_name, files) = preview_template(&opts).unwrap();
    assert_eq!(crate_name, "fancy-battery-pack");
    assert!(!files.is_empty());
    assert!(files.iter().any(|f| f.path == "Cargo.toml"));
    assert!(files.iter().any(|f| f.path == "src/main.rs"));
}
