use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use syn::{visit::Visit, Item, ItemUse, UseTree, Visibility};

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

struct ParsedModuleGraph {
    dependencies: BTreeMap<String, BTreeSet<String>>,
    exports: BTreeMap<String, BTreeSet<String>>,
}

impl ParsedModuleGraph {
    fn with_dependencies<I, E>(entries: I) -> Self
    where
        I: IntoIterator<Item = (&'static str, E)>,
        E: IntoIterator<Item = &'static str>,
    {
        let mut graph = Self {
            dependencies: BTreeMap::new(),
            exports: BTreeMap::new(),
        };
        for (module, edges) in entries {
            graph.dependencies.insert(
                module.to_owned(),
                edges.into_iter().map(str::to_owned).collect(),
            );
        }
        graph
    }

    fn with_exports<const N: usize, const M: usize>(
        entries: [(&'static str, [&'static str; M]); N],
    ) -> Self {
        let mut graph = Self {
            dependencies: BTreeMap::new(),
            exports: BTreeMap::new(),
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
            exports: BTreeMap::new(),
        };
        let mut files = Vec::new();
        collect_rs_files(root.as_ref(), &mut files)?;
        for file in files {
            let source =
                fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
            let parsed =
                syn::parse_file(&source).map_err(|e| format!("{}: {e}", file.display()))?;
            let module = module_name(&file);
            let mut visitor = ModuleVisitor {
                uses: Vec::new(),
                exports: Vec::new(),
                include_restricted: module == "services::proxy::response_body",
            };
            visitor.visit_file(&parsed);
            let deps = graph.dependencies.entry(module.clone()).or_default();
            deps.extend(
                visitor
                    .uses
                    .into_iter()
                    .map(|path| normalize_path(&path, &module)),
            );
            let exports = graph.exports.entry(module.clone()).or_default();
            exports.extend(visitor.exports);
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
                        visit(target, graph, path, cycles, expanded);
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

struct BoundaryManifest {
    allowed_exports: BTreeMap<String, Vec<String>>,
    legacy_cycle_baseline: BTreeSet<Vec<String>>,
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
        }
    }

    fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
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
        Ok(Self {
            allowed_exports,
            legacy_cycle_baseline,
        })
    }
}

struct ModuleVisitor {
    uses: Vec<String>,
    exports: Vec<String>,
    include_restricted: bool,
}

impl<'ast> Visit<'ast> for ModuleVisitor {
    fn visit_item(&mut self, item: &'ast Item) {
        if let Some(name) = public_item_name(item, self.include_restricted) {
            self.exports.push(name);
        }
        syn::visit::visit_item(self, item);
    }

    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        let mut paths = Vec::new();
        flatten_use_tree(&item.tree, String::new(), &mut paths);
        self.uses.extend(paths.iter().cloned());
        if matches!(item.vis, Visibility::Public(_)) {
            self.exports.extend(
                paths
                    .into_iter()
                    .filter_map(|path| path.rsplit("::").next().map(str::to_owned)),
            );
        }
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
    if !matches!(visibility, Visibility::Public(_))
        && !(include_restricted && matches!(visibility, Visibility::Restricted(_)))
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
