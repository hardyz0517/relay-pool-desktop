use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use syn::{
    parse::Parser, punctuated::Punctuated, visit::Visit, Item, ItemUse, Meta, Token, UseTree,
    Visibility,
};

#[test]
fn persistence_v2_dependency_edges_match_the_boundary_manifest() {
    let graph = ParsedModuleGraph::load("src").expect("parse Rust module graph");
    let manifest =
        BoundaryManifest::load("../docs/superpowers/audits/persistence-v2-boundary-manifest.json")
            .expect("load boundary manifest");

    graph.assert_no_dependency_cycles(&manifest);
    graph.assert_forbidden_prefix_absent("application", "tauri");
    graph.assert_forbidden_prefix_absent("application", "sqlx");
    graph.assert_forbidden_prefix_absent("commands", "sqlx");
    graph.assert_forbidden_prefix_absent("services::proxy", "persistence::stores");
    graph.assert_public_exports_equal(&manifest.allowed_exports);
    graph.assert_allowed_edges_registered(&manifest.allowed_edges);
    graph.assert_max_fan_in(&manifest.max_fan_in);
    graph.assert_max_fan_out(&manifest.max_fan_out);
    assert!(
        manifest.allowed_edges.iter().all(|edge| !edge.temporary),
        "temporary dependency edges must be removed after the V2 cutover"
    );
}

#[test]
fn legacy_persistence_is_absent_from_release_source() {
    let source_root = Path::new("src");
    let manifest =
        BoundaryManifest::load("../docs/superpowers/audits/persistence-v2-boundary-manifest.json")
            .expect("load boundary manifest");

    assert!(
        !source_root.join("services/database.rs").exists(),
        "legacy services/database.rs must not exist in release source"
    );
    assert!(
        manifest.temporary_legacy_consumers.is_empty(),
        "temporary legacy consumers must be empty after the V2 cutover"
    );
    assert!(
        manifest.temporary_legacy_paths.is_empty(),
        "temporary legacy paths must be empty after the V2 cutover"
    );

    let mut files = Vec::new();
    collect_rs_files(source_root, &mut files).expect("collect release Rust sources");
    let test_only_modules = test_only_file_modules(&files).expect("resolve test-only modules");
    for file in &files {
        if test_only_modules.contains(&module_name(file)) {
            continue;
        }
        let source = fs::read_to_string(file).expect("read release Rust source");
        assert!(
            !source_has_production_identifier(&source, "AppDatabase")
                .expect("parse release Rust source"),
            "legacy AppDatabase identifier remains in {}",
            file.display()
        );
    }

    let production_dependencies =
        cargo_metadata_production_dependencies(Path::new("Cargo.toml"), "relay-pool-desktop")
            .expect("read production dependencies through cargo metadata");
    assert!(
        !production_dependencies.contains("rusqlite"),
        "legacy rusqlite dependency must not remain in the production dependency graph"
    );

    for (target, allowed) in &manifest.allowed_consumers {
        let actual = external_consumers(source_root, target).expect("scan compatibility consumers");
        let expected = allowed.iter().cloned().collect::<BTreeSet<_>>();
        assert_eq!(
            actual, expected,
            "compatibility consumers differ from manifest for {target}"
        );
    }
}

#[test]
fn registered_legacy_cycle_passes() {
    let graph = ParsedModuleGraph::with_dependencies([
        ("commands", ["services::capture"]),
        ("services::capture", ["commands"]),
    ]);
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(
        BTreeMap::new(),
        [vec![
            "commands".to_owned(),
            "services::capture".to_owned(),
            "commands".to_owned(),
        ]],
    );

    graph.assert_no_dependency_cycles(&manifest);
}

#[test]
fn qualified_reference_within_the_same_module_is_not_a_dependency_cycle() {
    let graph = parsed_graph_from_source_modules(&[(
        "application::app_services",
        "struct AppServices; fn load(_: crate::application::app_services::AppServices) {}",
    )])
    .expect("parse same-module qualified path fixture");
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(BTreeMap::new(), []);

    graph.assert_no_dependency_cycles(&manifest);
}

#[test]
#[should_panic(expected = "V2 dependency cycle")]
fn registered_v2_cycle_still_fails() {
    let graph = ParsedModuleGraph::with_dependencies([
        ("application", ["persistence"]),
        ("persistence", ["application"]),
    ]);
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(
        BTreeMap::new(),
        [vec![
            "application".to_owned(),
            "persistence".to_owned(),
            "application".to_owned(),
        ]],
    );

    graph.assert_no_dependency_cycles(&manifest);
}

#[test]
#[should_panic(expected = "dependency cycle")]
fn unregistered_self_loop_fails() {
    let graph =
        ParsedModuleGraph::with_dependencies([("services::capture", ["services::capture"])]);
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(BTreeMap::new(), []);

    graph.assert_no_dependency_cycles(&manifest);
}

#[test]
#[should_panic(expected = "dependency cycle")]
fn unregistered_intra_root_cycle_fails() {
    let graph = ParsedModuleGraph::with_dependencies([
        ("commands", ["services::capture"]),
        ("services::capture", ["services::proxy"]),
        ("services::proxy", ["commands"]),
    ]);
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(BTreeMap::new(), []);

    graph.assert_no_dependency_cycles(&manifest);
}

#[test]
#[should_panic(expected = "dependency cycle")]
fn new_cycle_beyond_the_baseline_fails() {
    let graph = ParsedModuleGraph::with_dependencies(vec![
        ("commands", vec!["services::capture"]),
        ("services::capture", vec!["commands", "services::proxy"]),
        ("services::proxy", vec!["services::capture"]),
    ]);
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(
        BTreeMap::new(),
        [vec![
            "commands".to_owned(),
            "services::capture".to_owned(),
            "commands".to_owned(),
        ]],
    );

    graph.assert_no_dependency_cycles(&manifest);
}

#[test]
fn forbidden_prefix_logic_sees_tauri_and_sqlx() {
    let tauri_graph = ParsedModuleGraph::with_dependencies([("application", ["tauri::Manager"])]);
    let sqlx_graph = ParsedModuleGraph::with_dependencies([("application", ["sqlx::query"])]);

    assert!(std::panic::catch_unwind(|| {
        tauri_graph.assert_forbidden_prefix_absent("application", "tauri");
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| {
        sqlx_graph.assert_forbidden_prefix_absent("application", "sqlx");
    })
    .is_err());
}

#[test]
fn export_comparison_checks_every_manifest_export_key() {
    let graph = ParsedModuleGraph::with_exports([("application", ["kept"])]);
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(
        [
            ("application".to_owned(), vec!["kept".to_owned()]),
            ("persistence".to_owned(), vec!["missing".to_owned()]),
        ]
        .into_iter()
        .collect(),
        [],
    );

    let result = std::panic::catch_unwind(|| {
        graph.assert_public_exports_equal(&manifest.allowed_exports);
    });
    assert!(result.is_err());
}

#[test]
#[should_panic(expected = "public exports differ from manifest")]
fn unregistered_v2_public_export_module_fails() {
    let graph = ParsedModuleGraph::with_exports([("application::unregistered", ["LeakedExport"])]);
    let manifest = BoundaryManifest::with_exports_and_legacy_cycle_baseline(BTreeMap::new(), []);

    graph.assert_public_exports_equal(&manifest.allowed_exports);
}

#[test]
fn boundary_manifest_rejects_malformed_allowed_edges() {
    let path = write_manifest_fixture(|value| {
        value["allowed_edges"] = serde_json::json!([
            {"from": "application", "temporary": false}
        ]);
    });

    let result = BoundaryManifest::load(&path);
    fs::remove_file(path).expect("remove manifest fixture");
    let error = result
        .err()
        .expect("allowed_edges must be parsed and validated");

    assert!(error.contains("allowed_edges"), "unexpected error: {error}");
}

#[test]
fn boundary_manifest_rejects_malformed_fan_in_limit() {
    let fan_in_path = write_manifest_fixture(|value| {
        value["fan_in_baseline"] = serde_json::json!({
            "DataDirConfigV2": {"direct_consumer_modules": "two"}
        });
    });
    let fan_in_result = BoundaryManifest::load(&fan_in_path);
    fs::remove_file(fan_in_path).expect("remove fan-in fixture");
    let fan_in_error = fan_in_result
        .err()
        .expect("fan_in_baseline must be parsed and validated");
    assert!(
        fan_in_error.contains("fan_in_baseline"),
        "unexpected error: {fan_in_error}"
    );
}

#[test]
fn boundary_manifest_rejects_malformed_fan_out_limit() {
    let fan_out_path = write_manifest_fixture(|value| {
        value["fan_out_baseline"] = serde_json::json!({
            "application": {"direct_dependency_modules": -1}
        });
    });
    let fan_out_result = BoundaryManifest::load(&fan_out_path);
    fs::remove_file(fan_out_path).expect("remove fan-out fixture");
    let fan_out_error = fan_out_result
        .err()
        .expect("fan_out_baseline must be parsed and validated");
    assert!(
        fan_out_error.contains("fan_out_baseline"),
        "unexpected error: {fan_out_error}"
    );
}

#[test]
#[should_panic(expected = "unregistered dependency edge")]
fn unregistered_allowed_edge_target_fails_without_a_cycle() {
    let graph = ParsedModuleGraph::with_dependencies(vec![
        ("application", vec!["persistence::Store"]),
        ("commands", Vec::new()),
        ("persistence", Vec::new()),
    ]);
    let allowed = vec![AllowedEdge {
        from: "commands".to_owned(),
        to: "persistence".to_owned(),
        temporary: false,
    }];

    graph.assert_allowed_edges_registered(&allowed);
}

#[test]
#[should_panic(expected = "fan-in limit exceeded")]
fn fan_in_above_the_manifest_limit_fails() {
    let graph = ParsedModuleGraph::with_dependencies([
        ("application::alpha", ["persistence::SharedBoundary"]),
        ("commands::beta", ["persistence::SharedBoundary"]),
    ]);

    graph.assert_max_fan_in(&BTreeMap::from([(
        "persistence::SharedBoundary".to_owned(),
        1,
    )]));
}

#[test]
#[should_panic(expected = "fan-out limit exceeded")]
fn fan_out_above_the_manifest_limit_fails() {
    let graph = ParsedModuleGraph::with_dependencies(vec![
        (
            "application",
            vec!["persistence::Gateway", "services::ProviderPort"],
        ),
        ("persistence", Vec::new()),
        ("services", Vec::new()),
    ]);

    graph.assert_max_fan_out(&BTreeMap::from([("application".to_owned(), 1)]));
}

#[test]
fn app_database_ast_scan_ignores_literals_and_test_only_items() {
    assert!(
        !source_has_production_identifier(
            r#"const LEGACY_LABEL: &str = "AppDatabase";"#,
            "AppDatabase",
        )
        .expect("parse string-literal fixture"),
        "a string literal must not be treated as a production identifier"
    );
    assert!(
        !source_has_production_identifier("#[cfg(test)] struct AppDatabase;", "AppDatabase",)
            .expect("parse test-only fixture"),
        "a cfg(test) item must not be treated as release source"
    );
    assert!(
        source_has_production_identifier("struct AppDatabase;", "AppDatabase")
            .expect("parse identifier fixture"),
        "a production identifier must be detected"
    );
    assert!(
        source_has_production_identifier(
            "fn load(value: crate::legacy::AppDatabase) {}",
            "AppDatabase",
        )
        .expect("parse type-path fixture"),
        "a production type path must be detected"
    );
}

#[test]
fn cargo_metadata_dependency_scan_excludes_dev_dependencies() {
    let metadata = serde_json::json!({
        "packages": [{
            "name": "relay-pool-desktop",
            "dependencies": [
                {"name": "sqlx", "kind": null},
                {"name": "tauri-build", "kind": "build"},
                {"name": "rusqlite", "kind": "dev"}
            ]
        }]
    });

    let dependencies = production_dependency_names_from_metadata(&metadata, "relay-pool-desktop")
        .expect("parse cargo metadata fixture");

    assert_eq!(
        dependencies,
        BTreeSet::from(["sqlx".to_owned(), "tauri-build".to_owned()])
    );
}

#[test]
#[should_panic(expected = "forbidden edge")]
fn parser_records_qualified_expression_paths() {
    let graph = parsed_graph_from_source_modules(&[(
        "application",
        r#"fn run() { let _ = sqlx::query("SELECT 1"); }"#,
    )])
    .expect("parse qualified-expression fixture");

    graph.assert_forbidden_prefix_absent("application", "sqlx");
}

#[test]
fn parser_attributes_inline_module_paths_to_the_inline_module() {
    let graph = parsed_graph_from_source_modules(&[
        (
            "application",
            "mod child { use crate::persistence::Gateway; fn load(_: Gateway) {} }",
        ),
        ("persistence", "pub struct Gateway;"),
    ])
    .expect("parse inline-module fixture");

    assert!(
        graph
            .dependencies
            .get("application::child")
            .is_some_and(|paths| paths.contains("persistence::Gateway")),
        "inline-module dependency must belong to application::child"
    );
}

#[test]
#[should_panic(expected = "fan-in limit exceeded")]
fn qualified_type_path_counts_toward_its_owned_symbol_fan_in() {
    let graph = parsed_graph_from_source_modules(&[
        (
            "application",
            "fn load(_: crate::persistence::SharedBoundary) {}",
        ),
        ("persistence", "pub struct SharedBoundary;"),
    ])
    .expect("parse qualified type-path fixture");

    graph.assert_max_fan_in(&BTreeMap::from([(
        "persistence::SharedBoundary".to_owned(),
        0,
    )]));
}

#[test]
#[should_panic(expected = "fan-in limit exceeded")]
fn glob_import_and_unqualified_use_count_toward_owned_symbol_fan_in() {
    let graph = parsed_graph_from_source_modules(&[
        (
            "application",
            "use crate::persistence::*; fn load(_: SharedBoundary) {}",
        ),
        ("persistence", "pub struct SharedBoundary;"),
    ])
    .expect("parse glob fan-in fixture");

    graph.assert_max_fan_in(&BTreeMap::from([(
        "persistence::SharedBoundary".to_owned(),
        0,
    )]));
}

#[test]
fn same_name_symbol_in_another_owner_does_not_count_toward_fan_in() {
    let graph = parsed_graph_from_source_modules(&[
        (
            "application",
            "fn load(_: crate::services::SharedBoundary) {}",
        ),
        ("persistence", "pub struct SharedBoundary;"),
        ("services", "pub struct SharedBoundary;"),
    ])
    .expect("parse same-name symbol fixture");

    graph.assert_max_fan_in(&BTreeMap::from([(
        "persistence::SharedBoundary".to_owned(),
        0,
    )]));
}

#[test]
fn cfg_not_test_item_remains_in_the_release_ast() {
    assert!(source_has_production_identifier(
        "#[cfg(not(test))] struct AppDatabase;",
        "AppDatabase",
    )
    .expect("parse cfg(not(test)) fixture"));
}

#[test]
fn mixed_test_and_production_cfg_remains_in_the_release_ast() {
    assert!(source_has_production_identifier(
        "#[cfg(any(test, feature = \"production\"))] struct AppDatabase;",
        "AppDatabase",
    )
    .expect("parse mixed cfg fixture"));
}

#[test]
fn test_support_filename_requires_a_test_only_parent_module() {
    assert!(
        !module_file_is_definitely_test_only("mod test_support;", "test_support")
            .expect("parse production module declaration")
    );
    assert!(
        module_file_is_definitely_test_only("#[cfg(test)] mod test_support;", "test_support",)
            .expect("parse test-only module declaration")
    );
}

#[test]
#[should_panic(expected = "unregistered dependency edge")]
fn allowed_edges_reject_cross_boundary_targets_not_named_by_the_manifest() {
    let graph = ParsedModuleGraph::with_dependencies(vec![
        ("application", vec!["services::ProviderPort"]),
        ("commands", vec!["persistence::Gateway"]),
        ("persistence", Vec::new()),
        ("services", Vec::new()),
    ]);
    let allowed = vec![AllowedEdge {
        from: "commands".to_owned(),
        to: "persistence".to_owned(),
        temporary: false,
    }];

    graph.assert_allowed_edges_registered(&allowed);
}

#[test]
#[should_panic(expected = "stale dependency edge allowance")]
fn allowed_edges_reject_stale_entries() {
    let graph = ParsedModuleGraph::with_dependencies(vec![
        ("commands", vec!["persistence::Gateway"]),
        ("application", Vec::new()),
        ("persistence", Vec::new()),
    ]);
    let allowed = vec![
        AllowedEdge {
            from: "commands".to_owned(),
            to: "persistence".to_owned(),
            temporary: false,
        },
        AllowedEdge {
            from: "application".to_owned(),
            to: "persistence".to_owned(),
            temporary: false,
        },
    ];

    graph.assert_allowed_edges_registered(&allowed);
}

#[test]
fn registered_allowed_edge_passes() {
    let graph = ParsedModuleGraph::with_dependencies(vec![
        ("commands", vec!["persistence::Gateway"]),
        ("persistence", Vec::new()),
    ]);
    let allowed = vec![AllowedEdge {
        from: "commands".to_owned(),
        to: "persistence".to_owned(),
        temporary: false,
    }];

    graph.assert_allowed_edges_registered(&allowed);
}

#[test]
#[should_panic(expected = "fan-out limit exceeded")]
fn fan_out_root_limit_includes_descendant_modules() {
    let graph = ParsedModuleGraph::with_dependencies(vec![
        ("application", Vec::new()),
        ("application::child", vec!["persistence::Gateway"]),
        ("persistence", Vec::new()),
    ]);

    graph.assert_max_fan_out(&BTreeMap::from([("application".to_owned(), 0)]));
}

#[test]
fn compatibility_consumer_scan_ignores_cfg_test_paths() {
    let consumers = external_consumers_from_source_modules(
        &[
            (
                "application",
                "#[cfg(test)] fn inspect(_: crate::persistence::legacy_import::Legacy) {}",
            ),
            ("persistence::legacy_import", "pub struct Legacy;"),
        ],
        "persistence::legacy_import",
    )
    .expect("scan compatibility consumer fixture");

    assert!(consumers.is_empty(), "test-only consumer must be ignored");
}

#[test]
fn cargo_metadata_dependency_scan_preserves_rename_target_and_mixed_kinds() {
    let metadata = serde_json::json!({
        "packages": [{
            "name": "relay-pool-desktop",
            "dependencies": [
                {"name": "rusqlite", "rename": "sqlite_compat", "kind": null, "target": "cfg(windows)"},
                {"name": "sqlx", "kind": null, "target": "cfg(windows)"},
                {"name": "sqlx", "kind": "dev", "target": null},
                {"name": "tauri-build", "kind": "build", "target": "cfg(windows)"}
            ]
        }]
    });

    let dependencies = production_dependency_names_from_metadata(&metadata, "relay-pool-desktop")
        .expect("parse metadata rename/target fixture");

    assert_eq!(
        dependencies,
        BTreeSet::from([
            "rusqlite".to_owned(),
            "sqlx".to_owned(),
            "tauri-build".to_owned(),
        ])
    );
}

#[test]
fn cargo_metadata_dependency_scan_rejects_unknown_kinds() {
    let metadata = serde_json::json!({
        "packages": [{
            "name": "relay-pool-desktop",
            "dependencies": [{"name": "rusqlite", "kind": "future-kind"}]
        }]
    });

    let error = production_dependency_names_from_metadata(&metadata, "relay-pool-desktop")
        .expect_err("unknown dependency kind must fail closed");
    assert!(error.contains("future-kind"), "unexpected error: {error}");
}

#[test]
fn boundary_manifest_rejects_unknown_versions() {
    let path = write_manifest_fixture(|value| {
        value["version"] = serde_json::json!(999);
    });
    let result = BoundaryManifest::load(&path);
    fs::remove_file(path).expect("remove version fixture");

    let error = result
        .err()
        .expect("unknown manifest version must fail closed");
    assert!(error.contains("version"), "unexpected error: {error}");
}

fn write_manifest_fixture(mutate: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let source =
        fs::read_to_string("../docs/superpowers/audits/persistence-v2-boundary-manifest.json")
            .expect("read boundary manifest fixture source");
    let mut value: serde_json::Value = serde_json::from_str(&source).expect("parse manifest");
    mutate(&mut value);
    let path = std::env::temp_dir().join(format!(
        "relay-pool-boundary-manifest-{}-{}.json",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    fs::write(
        &path,
        serde_json::to_vec_pretty(&value).expect("encode manifest"),
    )
    .expect("write manifest fixture");
    path
}

fn source_has_production_identifier(source: &str, target: &str) -> Result<bool, String> {
    let parsed = syn::parse_file(source).map_err(|error| error.to_string())?;
    let mut visitor = ProductionIdentifierVisitor {
        target,
        found: false,
    };
    visitor.visit_file(&parsed);
    Ok(visitor.found)
}

fn production_dependency_names_from_metadata(
    metadata: &serde_json::Value,
    package_name: &str,
) -> Result<BTreeSet<String>, String> {
    let packages = metadata
        .get("packages")
        .and_then(serde_json::Value::as_array)
        .ok_or("cargo metadata packages missing")?;
    let package = packages
        .iter()
        .find(|package| {
            package.get("name").and_then(serde_json::Value::as_str) == Some(package_name)
        })
        .ok_or_else(|| format!("cargo metadata package {package_name} missing"))?;
    let dependencies = package
        .get("dependencies")
        .and_then(serde_json::Value::as_array)
        .ok_or("cargo metadata dependencies missing")?;
    let mut production = BTreeSet::new();
    for dependency in dependencies {
        let include = match dependency.get("kind") {
            None | Some(serde_json::Value::Null) => true,
            Some(serde_json::Value::String(kind)) if kind == "build" => true,
            Some(serde_json::Value::String(kind)) if kind == "dev" => false,
            Some(kind) => {
                return Err(format!(
                    "cargo metadata dependency kind is unsupported: {kind}"
                ));
            }
        };
        if include {
            let name = dependency
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or("cargo metadata dependency name missing")?;
            production.insert(name.to_owned());
        }
    }
    Ok(production)
}

fn cargo_metadata_production_dependencies(
    manifest_path: &Path,
    package_name: &str,
) -> Result<BTreeSet<String>, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--no-deps",
            "--locked",
            "--manifest-path",
        ])
        .arg(manifest_path)
        .output()
        .map_err(|error| format!("failed to execute cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid cargo metadata JSON: {error}"))?;
    production_dependency_names_from_metadata(&metadata, package_name)
}

fn parsed_graph_from_source_modules(modules: &[(&str, &str)]) -> Result<ParsedModuleGraph, String> {
    let mut graph = ParsedModuleGraph {
        dependencies: BTreeMap::new(),
        glob_imports: BTreeMap::new(),
        exports: BTreeMap::new(),
        explicit_self_loops: BTreeSet::new(),
    };
    for (module, source) in modules {
        let parsed = syn::parse_file(source).map_err(|error| error.to_string())?;
        collect_parsed_module(&mut graph, module, &parsed.items);
    }
    Ok(graph)
}

fn module_file_is_definitely_test_only(
    parent_source: &str,
    module_name: &str,
) -> Result<bool, String> {
    let parsed = syn::parse_file(parent_source).map_err(|error| error.to_string())?;
    Ok(parsed.items.iter().any(|item| {
        matches!(item, Item::Mod(item_mod) if item_mod.ident == module_name)
            && item_is_definitely_test_only(item)
    }))
}

fn external_consumers_from_source_modules(
    modules: &[(&str, &str)],
    target: &str,
) -> Result<BTreeSet<String>, String> {
    Ok(parsed_graph_from_source_modules(modules)?.external_consumers_of(target))
}

struct ProductionIdentifierVisitor<'a> {
    target: &'a str,
    found: bool,
}

impl<'ast> Visit<'ast> for ProductionIdentifierVisitor<'_> {
    fn visit_item(&mut self, item: &'ast Item) {
        if item_is_definitely_test_only(item) {
            return;
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_ident(&mut self, ident: &'ast syn::Ident) {
        if ident == self.target {
            self.found = true;
        }
    }
}

struct ParsedModuleGraph {
    dependencies: BTreeMap<String, BTreeSet<String>>,
    glob_imports: BTreeMap<String, BTreeSet<String>>,
    exports: BTreeMap<String, BTreeSet<String>>,
    explicit_self_loops: BTreeSet<String>,
}

impl ParsedModuleGraph {
    fn with_dependencies<I, E>(entries: I) -> Self
    where
        I: IntoIterator<Item = (&'static str, E)>,
        E: IntoIterator<Item = &'static str>,
    {
        let mut graph = Self {
            dependencies: BTreeMap::new(),
            glob_imports: BTreeMap::new(),
            exports: BTreeMap::new(),
            explicit_self_loops: BTreeSet::new(),
        };
        for (module, edges) in entries {
            let edges = edges
                .into_iter()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>();
            if edges.contains(module) {
                graph.explicit_self_loops.insert(module.to_owned());
            }
            graph.dependencies.insert(module.to_owned(), edges);
        }
        graph
    }

    fn with_exports<const N: usize, const M: usize>(
        entries: [(&'static str, [&'static str; M]); N],
    ) -> Self {
        let mut graph = Self {
            dependencies: BTreeMap::new(),
            glob_imports: BTreeMap::new(),
            exports: BTreeMap::new(),
            explicit_self_loops: BTreeSet::new(),
        };
        for (module, exports) in entries {
            graph.exports.insert(
                module.to_owned(),
                exports.into_iter().map(str::to_owned).collect(),
            );
        }
        graph
    }

    fn load(root: impl AsRef<Path>) -> Result<Self, String> {
        let mut graph = Self {
            dependencies: BTreeMap::new(),
            glob_imports: BTreeMap::new(),
            exports: BTreeMap::new(),
            explicit_self_loops: BTreeSet::new(),
        };
        let mut files = Vec::new();
        collect_rs_files(root.as_ref(), &mut files)?;
        let test_only_modules = test_only_file_modules(&files)?;
        for file in files {
            let module = module_name(&file);
            if test_only_modules.contains(&module) {
                continue;
            }
            let source =
                fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let parsed =
                syn::parse_file(&source).map_err(|e| format!("{}: {e}", file.display()))?;
            collect_parsed_module(&mut graph, &module, &parsed.items);
        }
        Ok(graph)
    }

    fn assert_no_dependency_cycles(&self, manifest: &BoundaryManifest) {
        let detected = self.detect_dependency_cycles();
        let allowed = manifest
            .legacy_cycle_baseline
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        for cycle in &allowed {
            assert!(
                !contains_v2_module(cycle),
                "V2 dependency cycle baseline entry is forbidden: {}",
                cycle.join(" -> ")
            );
        }
        for cycle in &detected {
            assert!(
                !contains_v2_module(cycle),
                "V2 dependency cycle: {}",
                cycle.join(" -> ")
            );
            assert!(
                allowed.contains(cycle),
                "dependency cycle: {}",
                cycle.join(" -> ")
            );
        }
        assert_eq!(
            detected, allowed,
            "dependency cycle baseline differs from detected cycles"
        );
    }

    fn detect_dependency_cycles(&self) -> BTreeSet<Vec<String>> {
        fn visit(
            node: &str,
            graph: &BTreeMap<String, BTreeSet<String>>,
            explicit_self_loops: &BTreeSet<String>,
            path: &mut Vec<String>,
            cycles: &mut BTreeSet<Vec<String>>,
            expanded: &mut BTreeSet<String>,
        ) {
            if path.contains(&node.to_string()) {
                let start = path.iter().position(|entry| entry == node).unwrap_or(0);
                let mut cycle = path[start..].to_vec();
                cycle.push(node.to_string());
                cycles.insert(canonicalize_cycle(cycle));
                return;
            }
            if !expanded.insert(node.to_string()) {
                return;
            }
            path.push(node.to_string());
            if let Some(edges) = graph.get(node) {
                for edge in edges {
                    if let Some(target) = resolve_target(edge, graph) {
                        if target == node && !explicit_self_loops.contains(node) {
                            continue;
                        }
                        visit(target, graph, explicit_self_loops, path, cycles, expanded);
                    }
                }
            }
            path.pop();
        }
        let mut cycles = BTreeSet::new();
        let mut expanded = BTreeSet::new();
        let mut path = Vec::new();
        for node in self.dependencies.keys() {
            visit(
                node,
                &self.dependencies,
                &self.explicit_self_loops,
                &mut path,
                &mut cycles,
                &mut expanded,
            );
        }
        cycles
    }

    fn assert_forbidden_prefix_absent(&self, from: &str, forbidden: &str) {
        for (module, paths) in &self.dependencies {
            if module == from || module.starts_with(&format!("{from}::")) {
                if let Some(path) = paths.iter().find(|path| path.starts_with(forbidden)) {
                    panic!("forbidden edge {module} -> {path}");
                }
            }
        }
    }

    fn assert_public_exports_equal(&self, allowed: &BTreeMap<String, Vec<String>>) {
        let actual = self.scoped_public_exports(allowed);
        let expected = allowed
            .iter()
            .filter(|(_, exports)| !exports.is_empty())
            .map(|(module, exports)| {
                (
                    module.clone(),
                    exports.iter().cloned().collect::<BTreeSet<_>>(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(actual, expected, "public exports differ from manifest");
    }

    fn assert_allowed_edges_registered(&self, allowed: &[AllowedEdge]) {
        let controlled_targets = allowed
            .iter()
            .map(|edge| edge.to.as_str())
            .collect::<BTreeSet<_>>();
        let actual = self.resolved_module_edges();
        let missing = actual
            .iter()
            .filter(|(from, to)| {
                !module_is_within(from, to)
                    && (boundary_owner(from) != boundary_owner(to)
                        || controlled_targets.contains(to.as_str()))
            })
            .filter(|(from, to)| !allowed.iter().any(|edge| edge_matches(edge, from, to)))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert!(
            missing.is_empty(),
            "unregistered dependency edge(s): {missing:?}"
        );

        let stale = allowed
            .iter()
            .filter(|edge| !actual.iter().any(|(from, to)| edge_matches(edge, from, to)))
            .cloned()
            .collect::<BTreeSet<_>>();
        assert!(
            stale.is_empty(),
            "stale dependency edge allowance(s): {stale:?}"
        );
    }

    fn assert_max_fan_in(&self, limits: &BTreeMap<String, usize>) {
        for (owned_symbol, maximum) in limits {
            let (owner, leaf) = owned_symbol
                .rsplit_once("::")
                .unwrap_or(("", owned_symbol.as_str()));
            let consumers = self
                .dependencies
                .iter()
                .filter(|(module, paths)| {
                    let qualified = paths.iter().any(|path| {
                        path == owned_symbol || path.starts_with(&format!("{owned_symbol}::"))
                    });
                    let imported_by_glob = !owner.is_empty()
                        && self.glob_imports.get(*module).is_some_and(|imports| {
                            imports.contains(owner)
                                && paths.iter().any(|path| {
                                    path == leaf || path.starts_with(&format!("{leaf}::"))
                                })
                        });
                    qualified || imported_by_glob
                })
                .map(|(module, _)| module)
                .collect::<BTreeSet<_>>();
            assert!(
                consumers.len() <= *maximum,
                "fan-in limit exceeded for {owned_symbol}: {} direct consumer modules > {maximum}: {consumers:?}",
                consumers.len()
            );
        }
    }

    fn assert_max_fan_out(&self, limits: &BTreeMap<String, usize>) {
        let resolved = self.resolved_module_edges();
        for (module, maximum) in limits {
            let dependencies = resolved
                .iter()
                .filter(|(from, _)| module_is_within(from, module))
                .map(|(_, to)| to)
                .collect::<BTreeSet<_>>();
            assert!(
                dependencies.len() <= *maximum,
                "fan-out limit exceeded for {module}: {} direct dependency modules > {maximum}: {dependencies:?}",
                dependencies.len()
            );
        }
    }

    fn resolved_module_edges(&self) -> BTreeSet<(String, String)> {
        self.dependencies
            .iter()
            .flat_map(|(from, paths)| {
                paths.iter().filter_map(|path| {
                    resolve_target(path, &self.dependencies).map(|to| (from.clone(), to.to_owned()))
                })
            })
            .filter(|(from, to)| from != to)
            .collect()
    }

    fn external_consumers_of(&self, target: &str) -> BTreeSet<String> {
        self.dependencies
            .iter()
            .filter(|(module, _)| !module_is_within(module, target))
            .filter(|(_, paths)| {
                paths
                    .iter()
                    .any(|path| path == target || path.starts_with(&format!("{target}::")))
            })
            .map(|(module, _)| module.clone())
            .collect()
    }

    fn scoped_public_exports(
        &self,
        allowed: &BTreeMap<String, Vec<String>>,
    ) -> BTreeMap<String, BTreeSet<String>> {
        self.exports
            .iter()
            .filter(|(module, exports)| {
                !exports.is_empty()
                    && (allowed.contains_key(*module)
                        || module.as_str() == "application"
                        || module.starts_with("application::")
                        || module.as_str() == "persistence"
                        || module.starts_with("persistence::"))
            })
            .map(|(module, exports)| (module.clone(), exports.clone()))
            .collect()
    }
}

fn module_is_within(module: &str, owner: &str) -> bool {
    module == owner || module.starts_with(&format!("{owner}::"))
}

fn boundary_owner(module: &str) -> &str {
    if let Some(rest) = module.strip_prefix("services::") {
        let domain = rest.split_once("::").map_or(rest, |(domain, _)| domain);
        return &module[.."services::".len() + domain.len()];
    }
    module.split_once("::").map_or(module, |(root, _)| root)
}

fn edge_matches(edge: &AllowedEdge, from: &str, to: &str) -> bool {
    module_is_within(from, &edge.from) && to == edge.to
}

struct BoundaryManifest {
    allowed_exports: BTreeMap<String, Vec<String>>,
    legacy_cycle_baseline: BTreeSet<Vec<String>>,
    allowed_edges: Vec<AllowedEdge>,
    max_fan_in: BTreeMap<String, usize>,
    max_fan_out: BTreeMap<String, usize>,
    allowed_consumers: BTreeMap<String, Vec<String>>,
    temporary_legacy_consumers: Vec<serde_json::Value>,
    temporary_legacy_paths: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct AllowedEdge {
    from: String,
    to: String,
    temporary: bool,
}

impl BoundaryManifest {
    fn with_exports_and_legacy_cycle_baseline<const N: usize>(
        allowed_exports: BTreeMap<String, Vec<String>>,
        legacy_cycle_baseline: [Vec<String>; N],
    ) -> Self {
        Self {
            allowed_exports,
            legacy_cycle_baseline: legacy_cycle_baseline
                .into_iter()
                .map(canonicalize_cycle)
                .collect(),
            allowed_edges: Vec::new(),
            max_fan_in: BTreeMap::new(),
            max_fan_out: BTreeMap::new(),
            allowed_consumers: BTreeMap::new(),
            temporary_legacy_consumers: Vec::new(),
            temporary_legacy_paths: Vec::new(),
        }
    }

    fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        if value.get("version").and_then(serde_json::Value::as_u64) != Some(2) {
            return Err("unsupported boundary manifest version; expected version 2".to_owned());
        }
        let allowed_exports = value
            .get("allowed_exports")
            .and_then(serde_json::Value::as_object)
            .ok_or("allowed_exports missing")?
            .iter()
            .map(|(module, values)| {
                let entries = values
                    .as_array()
                    .ok_or("exports must be arrays")?
                    .iter()
                    .map(|v| v.as_str().map(str::to_owned).ok_or("export must be string"))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok((module.clone(), entries))
            })
            .collect::<Result<BTreeMap<_, _>, &str>>()?;
        let legacy_cycle_baseline = value
            .get("legacy_cycle_baseline")
            .and_then(serde_json::Value::as_array)
            .ok_or("legacy_cycle_baseline missing")?
            .iter()
            .map(|cycle| {
                let entries = cycle
                    .as_array()
                    .ok_or("cycle must be arrays")?
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .ok_or("cycle entry must be string")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(canonicalize_cycle(entries))
            })
            .collect::<Result<BTreeSet<_>, &str>>()?;
        let allowed_edges = parse_allowed_edges(&value)?;
        let max_fan_in = parse_fan_limits(&value, "fan_in_baseline", "direct_consumer_modules")?;
        let max_fan_out =
            parse_fan_limits(&value, "fan_out_baseline", "direct_dependency_modules")?;
        let allowed_consumers = string_array_map(&value, "allowed_consumers")?;
        let temporary_legacy_consumers = value
            .get("temporary_legacy_consumers")
            .and_then(serde_json::Value::as_array)
            .ok_or("temporary_legacy_consumers missing")?
            .clone();
        let temporary_legacy_paths = value
            .get("temporary_legacy_paths")
            .and_then(serde_json::Value::as_array)
            .ok_or("temporary_legacy_paths missing")?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or("temporary legacy path must be string")
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            allowed_exports,
            legacy_cycle_baseline,
            allowed_edges,
            max_fan_in,
            max_fan_out,
            allowed_consumers,
            temporary_legacy_consumers,
            temporary_legacy_paths,
        })
    }
}

fn parse_allowed_edges(value: &serde_json::Value) -> Result<Vec<AllowedEdge>, String> {
    let entries = value
        .get("allowed_edges")
        .and_then(serde_json::Value::as_array)
        .ok_or("allowed_edges missing or not an array")?;
    let mut seen = BTreeSet::new();
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let object = entry
                .as_object()
                .ok_or_else(|| format!("allowed_edges[{index}] must be an object"))?;
            let from = non_empty_string_field(object, "from", &format!("allowed_edges[{index}]"))?;
            let to = non_empty_string_field(object, "to", &format!("allowed_edges[{index}]"))?;
            let temporary = object
                .get("temporary")
                .and_then(serde_json::Value::as_bool)
                .ok_or_else(|| format!("allowed_edges[{index}].temporary must be a boolean"))?;
            if !seen.insert((from.clone(), to.clone())) {
                return Err(format!(
                    "allowed_edges contains duplicate edge {from} -> {to}"
                ));
            }
            Ok(AllowedEdge {
                from,
                to,
                temporary,
            })
        })
        .collect()
}

fn parse_fan_limits(
    value: &serde_json::Value,
    field: &str,
    count_field: &str,
) -> Result<BTreeMap<String, usize>, String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| format!("{field} missing or not an object"))?
        .iter()
        .map(|(owner, limit)| {
            let count = limit
                .as_object()
                .and_then(|object| object.get(count_field))
                .and_then(serde_json::Value::as_u64)
                .and_then(|count| usize::try_from(count).ok())
                .ok_or_else(|| {
                    format!("{field}.{owner}.{count_field} must be a non-negative integer")
                })?;
            Ok((owner.clone(), count))
        })
        .collect()
}

fn non_empty_string_field(
    object: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    context: &str,
) -> Result<String, String> {
    object
        .get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{context}.{field} must be a non-empty string"))
}

fn string_array_map(
    value: &serde_json::Value,
    field: &str,
) -> Result<BTreeMap<String, Vec<String>>, String> {
    value
        .get(field)
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| format!("{field} missing"))?
        .iter()
        .map(|(key, values)| {
            let entries = values
                .as_array()
                .ok_or_else(|| format!("{field}.{key} must be an array"))?
                .iter()
                .map(|entry| {
                    entry
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| format!("{field}.{key} entry must be string"))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok((key.clone(), entries))
        })
        .collect()
}

fn collect_parsed_module(graph: &mut ParsedModuleGraph, module: &str, items: &[Item]) {
    graph.dependencies.entry(module.to_owned()).or_default();
    graph.glob_imports.entry(module.to_owned()).or_default();
    graph.exports.entry(module.to_owned()).or_default();

    for item in items {
        if item_is_definitely_test_only(item) {
            continue;
        }
        if let Item::Mod(item_mod) = item {
            if let Some((_, child_items)) = &item_mod.content {
                let child_module = format!("{module}::{}", item_mod.ident);
                collect_parsed_module(graph, &child_module, child_items);
                if matches!(item_mod.vis, Visibility::Public(_)) {
                    graph
                        .exports
                        .entry(module.to_owned())
                        .or_default()
                        .insert(item_mod.ident.to_string());
                }
                continue;
            }
        }

        let mut visitor = ModuleVisitor {
            uses: Vec::new(),
            paths: Vec::new(),
            glob_imports: Vec::new(),
            exports: Vec::new(),
            include_restricted: module == "services::proxy::response_body",
        };
        visitor.visit_item(item);
        let dependencies = graph.dependencies.entry(module.to_owned()).or_default();
        dependencies.extend(
            visitor
                .uses
                .into_iter()
                .chain(visitor.paths)
                .map(|path| normalize_path(&path, module))
                .filter(|path| !path.is_empty()),
        );
        graph
            .glob_imports
            .entry(module.to_owned())
            .or_default()
            .extend(
                visitor
                    .glob_imports
                    .into_iter()
                    .map(|path| normalize_path(&path, module)),
            );
        graph
            .exports
            .entry(module.to_owned())
            .or_default()
            .extend(visitor.exports);
    }
}

struct ModuleVisitor {
    uses: Vec<String>,
    paths: Vec<String>,
    glob_imports: Vec<String>,
    exports: Vec<String>,
    include_restricted: bool,
}

impl<'ast> Visit<'ast> for ModuleVisitor {
    fn visit_item(&mut self, item: &'ast Item) {
        if item_is_definitely_test_only(item) {
            return;
        }
        if let Some(name) = public_item_name(item, self.include_restricted) {
            self.exports.push(name);
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        let mut paths = Vec::new();
        flatten_use_tree(&item.tree, String::new(), &mut paths);
        self.uses.extend(paths.iter().cloned());
        collect_glob_prefixes(&item.tree, String::new(), &mut self.glob_imports);
        if matches!(item.vis, Visibility::Public(_)) {
            self.exports.extend(
                paths
                    .into_iter()
                    .filter_map(|path| path.rsplit("::").next().map(str::to_owned)),
            );
        }
    }

    fn visit_path(&mut self, path: &'ast syn::Path) {
        self.paths.push(
            path.segments
                .iter()
                .map(|segment| segment.ident.to_string())
                .collect::<Vec<_>>()
                .join("::"),
        );
        syn::visit::visit_path(self, path);
    }
}

fn collect_glob_prefixes(tree: &UseTree, prefix: String, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(path) => {
            collect_glob_prefixes(&path.tree, join_path(prefix, path.ident.to_string()), out)
        }
        UseTree::Glob(_) => out.push(prefix),
        UseTree::Group(group) => {
            for item in &group.items {
                collect_glob_prefixes(item, prefix.clone(), out);
            }
        }
        UseTree::Name(_) | UseTree::Rename(_) => {}
    }
}

fn public_item_name(item: &Item, include_restricted: bool) -> Option<String> {
    let visibility = match item {
        Item::Const(item) => &item.vis,
        Item::Enum(item) => &item.vis,
        Item::Fn(item) => &item.vis,
        Item::Mod(item) => &item.vis,
        Item::Struct(item) => &item.vis,
        Item::Trait(item) => &item.vis,
        Item::Type(item) => &item.vis,
        _ => return None,
    };
    if !(matches!(visibility, Visibility::Public(_))
        || include_restricted && matches!(visibility, Visibility::Restricted(_)))
    {
        return None;
    }
    Some(match item {
        Item::Const(item) => item.ident.to_string(),
        Item::Enum(item) => item.ident.to_string(),
        Item::Fn(item) => item.sig.ident.to_string(),
        Item::Mod(item) => item.ident.to_string(),
        Item::Struct(item) => item.ident.to_string(),
        Item::Trait(item) => item.ident.to_string(),
        Item::Type(item) => item.ident.to_string(),
        _ => unreachable!(),
    })
}

fn flatten_use_tree(tree: &UseTree, prefix: String, out: &mut Vec<String>) {
    match tree {
        UseTree::Path(path) => {
            flatten_use_tree(&path.tree, join_path(prefix, path.ident.to_string()), out)
        }
        UseTree::Name(name) => out.push(join_path(prefix, name.ident.to_string())),
        UseTree::Rename(rename) => out.push(join_path(prefix, rename.ident.to_string())),
        UseTree::Glob(_) => out.push(prefix),
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use_tree(item, prefix.clone(), out)
            }
        }
    }
}

fn join_path(prefix: String, part: String) -> String {
    if prefix.is_empty() {
        part
    } else {
        format!("{prefix}::{part}")
    }
}

fn normalize_path(path: &str, module: &str) -> String {
    let mut parts: Vec<&str> = path.split("::").collect();
    if parts.first() == Some(&"crate") {
        parts.remove(0);
    } else if parts.first() == Some(&"self") {
        parts.remove(0);
        let mut base: Vec<&str> = module.split("::").collect();
        base.extend(parts);
        parts = base;
    } else if parts.first() == Some(&"super") {
        let mut base: Vec<&str> = module.split("::").collect();
        base.pop();
        parts.remove(0);
        base.extend(parts);
        parts = base;
    }
    parts.join("::")
}

fn resolve_target<'a>(
    edge: &'a str,
    graph: &'a BTreeMap<String, BTreeSet<String>>,
) -> Option<&'a str> {
    let mut candidate = edge;
    while !candidate.is_empty() {
        if graph.contains_key(candidate) {
            return Some(candidate);
        }
        candidate = candidate
            .rsplit_once("::")
            .map(|(prefix, _)| prefix)
            .unwrap_or("");
    }
    None
}

fn canonicalize_cycle(mut cycle: Vec<String>) -> Vec<String> {
    if cycle.is_empty() {
        return cycle;
    }
    if cycle.first() != cycle.last() {
        if let Some(first) = cycle.first().cloned() {
            cycle.push(first);
        }
    }
    let body = cycle[..cycle.len() - 1].to_vec();
    let mut best = body.clone();
    for i in 0..body.len() {
        let mut rotated = body[i..].to_vec();
        rotated.extend_from_slice(&body[..i]);
        if rotated < best {
            best = rotated;
        }
    }
    let mut result = best;
    result.push(result[0].clone());
    result
}

fn contains_v2_module(cycle: &[String]) -> bool {
    cycle.iter().any(|module| {
        module == "application"
            || module.starts_with("application::")
            || module == "persistence"
            || module.starts_with("persistence::")
    })
}

#[derive(Clone, Copy)]
struct CfgPossibilities {
    can_be_true: bool,
    can_be_false: bool,
}

impl CfgPossibilities {
    const TRUE: Self = Self {
        can_be_true: true,
        can_be_false: false,
    };
    const FALSE: Self = Self {
        can_be_true: false,
        can_be_false: true,
    };
    const UNKNOWN: Self = Self {
        can_be_true: true,
        can_be_false: true,
    };

    fn and(self, other: Self) -> Self {
        Self {
            can_be_true: self.can_be_true && other.can_be_true,
            can_be_false: self.can_be_false || other.can_be_false,
        }
    }

    fn or(self, other: Self) -> Self {
        Self {
            can_be_true: self.can_be_true || other.can_be_true,
            can_be_false: self.can_be_false && other.can_be_false,
        }
    }

    fn not(self) -> Self {
        Self {
            can_be_true: self.can_be_false,
            can_be_false: self.can_be_true,
        }
    }
}

fn item_is_definitely_test_only(item: &Item) -> bool {
    item_attributes(item).iter().any(|attribute| {
        if !attribute.path().is_ident("cfg") {
            return false;
        }
        let Ok(list) = attribute.meta.require_list() else {
            return false;
        };
        let Ok(predicates) =
            Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())
        else {
            return false;
        };
        predicates.len() == 1
            && predicates
                .first()
                .is_some_and(|predicate| !cfg_possibilities(predicate).can_be_true)
    })
}

fn item_attributes(item: &Item) -> &[syn::Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

fn cfg_possibilities(meta: &Meta) -> CfgPossibilities {
    match meta {
        Meta::Path(path) if path.is_ident("test") => CfgPossibilities::FALSE,
        Meta::Path(_) | Meta::NameValue(_) => CfgPossibilities::UNKNOWN,
        Meta::List(list) => {
            let Ok(nested) =
                Punctuated::<Meta, Token![,]>::parse_terminated.parse2(list.tokens.clone())
            else {
                return CfgPossibilities::UNKNOWN;
            };
            if list.path.is_ident("all") {
                nested.iter().fold(CfgPossibilities::TRUE, |value, meta| {
                    value.and(cfg_possibilities(meta))
                })
            } else if list.path.is_ident("any") {
                nested.iter().fold(CfgPossibilities::FALSE, |value, meta| {
                    value.or(cfg_possibilities(meta))
                })
            } else if list.path.is_ident("not") && nested.len() == 1 {
                cfg_possibilities(nested.first().expect("one not predicate")).not()
            } else {
                CfgPossibilities::UNKNOWN
            }
        }
    }
}

fn test_only_file_modules(files: &[PathBuf]) -> Result<BTreeSet<String>, String> {
    let mut parsed_modules = BTreeMap::new();
    for file in files {
        let source =
            fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
        let parsed =
            syn::parse_file(&source).map_err(|error| format!("{}: {error}", file.display()))?;
        parsed_modules.insert(module_name(file), parsed);
    }

    fn determine(
        module: &str,
        parsed_modules: &BTreeMap<String, syn::File>,
        memo: &mut BTreeMap<String, bool>,
        visiting: &mut BTreeSet<String>,
    ) -> bool {
        if module == "lib" || module == "main" {
            return false;
        }
        if let Some(result) = memo.get(module) {
            return *result;
        }
        if !visiting.insert(module.to_owned()) {
            return false;
        }

        let (parents, child) = match module.rsplit_once("::") {
            Some((parent, child)) => (vec![parent], child),
            None => (vec!["lib", "main"], module),
        };
        let mut saw_declaration = false;
        let mut saw_release_declaration = false;
        for parent in parents {
            let parent_test_only = determine(parent, parsed_modules, memo, visiting);
            let Some(parent_file) = parsed_modules.get(parent) else {
                continue;
            };
            for item in &parent_file.items {
                if matches!(item, Item::Mod(item_mod) if item_mod.ident == child) {
                    saw_declaration = true;
                    if !parent_test_only && !item_is_definitely_test_only(item) {
                        saw_release_declaration = true;
                    }
                }
            }
        }
        visiting.remove(module);
        let result = saw_declaration && !saw_release_declaration;
        memo.insert(module.to_owned(), result);
        result
    }

    let mut memo = BTreeMap::new();
    let mut test_only = BTreeSet::new();
    for module in parsed_modules.keys() {
        if determine(module, &parsed_modules, &mut memo, &mut BTreeSet::new()) {
            test_only.insert(module.clone());
        }
    }
    Ok(test_only)
}

fn external_consumers(root: &Path, target: &str) -> Result<BTreeSet<String>, String> {
    Ok(ParsedModuleGraph::load(root)?.external_consumers_of(target))
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.is_dir() {
            collect_rs_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn module_name(file: &Path) -> String {
    let relative = file.strip_prefix("src").unwrap_or(file);
    let mut parts: Vec<_> = relative
        .iter()
        .map(|part| part.to_string_lossy().into_owned())
        .collect();
    if let Some(last) = parts.last_mut() {
        if last == "lib.rs" {
            parts.pop();
            parts.push("lib".into());
        } else if last == "mod.rs" {
            parts.pop();
        } else {
            *last = last.trim_end_matches(".rs").to_owned();
        }
    }
    parts.join("::")
}
