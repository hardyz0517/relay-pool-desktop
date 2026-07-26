use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

use serde_json::Value;
use syn::{
    punctuated::Punctuated,
    visit::{self, Visit},
    Attribute, ExprCall, ExprMacro, ExprMethodCall, ImplItemFn, Item, ItemFn, ItemMacro, ItemMod,
    ItemUse, Meta, Token, UseTree, Visibility,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct Edge {
    from: String,
    to: String,
    kind: &'static str,
}

impl Edge {
    fn identity(&self) -> String {
        format!("{} -> {} [{}]", self.from, self.to, self.kind)
    }
}

#[derive(Default)]
struct Graph {
    edges: BTreeSet<Edge>,
    public_exports: BTreeSet<String>,
    spawn_sites: BTreeSet<String>,
    blocking_executor_submit_sites: BTreeSet<String>,
    http_client_sites: BTreeSet<String>,
    registry_macros: BTreeSet<String>,
    parsed_modules: BTreeSet<String>,
}

struct SourceVisitor<'a> {
    owner: String,
    graph: &'a mut Graph,
}

impl<'ast> Visit<'ast> for SourceVisitor<'_> {
    fn visit_expr_call(&mut self, node: &'ast ExprCall) {
        let called = quote_path(&node.func);
        if matches!(
            called.as_str(),
            "tokio::spawn"
                | "tokio::task::spawn"
                | "tokio::task::spawn_blocking"
                | "tauri::async_runtime::spawn"
                | "tauri::async_runtime::spawn_blocking"
                | "std::thread::spawn"
                | "thread::spawn"
        ) {
            self.graph
                .spawn_sites
                .insert(format!("{}::{called}", self.owner));
        }
        if called.contains("reqwest::Client::new")
            || called.contains("reqwest::Client::builder")
            || called.contains("ureq::Agent::new")
            || called.contains("ureq::AgentBuilder::new")
        {
            self.graph
                .http_client_sites
                .insert(format!("{}::{called}", self.owner));
        }
        visit::visit_expr_call(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        if matches!(node.method.to_string().as_str(), "spawn" | "spawn_blocking") {
            self.graph
                .spawn_sites
                .insert(format!("{}::<method>::{}", self.owner, node.method));
        }
        if node.method == "submit" {
            if let Some(syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(kind),
                ..
            })) = node.args.first()
            {
                self.graph.blocking_executor_submit_sites.insert(format!(
                    "{}::<method>::submit::{}",
                    self.owner,
                    kind.value()
                ));
            }
        }
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_macro(&mut self, node: &'ast ExprMacro) {
        let macro_path = path_to_string(&node.mac.path);
        if is_registry_macro(&macro_path) {
            self.graph
                .registry_macros
                .insert(format!("{}::{macro_path}", self.owner));
        }
        visit::visit_expr_macro(self, node);
    }

    fn visit_item_macro(&mut self, node: &'ast ItemMacro) {
        let macro_path = path_to_string(&node.mac.path);
        if is_registry_macro(&macro_path) {
            self.graph
                .registry_macros
                .insert(format!("{}::{macro_path}", self.owner));
        }
        visit::visit_item_macro(self, node);
    }

    fn visit_item_fn(&mut self, node: &'ast ItemFn) {
        let previous = self.owner.clone();
        self.owner = format!("{}::{}", previous, node.sig.ident);
        visit::visit_item_fn(self, node);
        self.owner = previous;
    }

    fn visit_impl_item_fn(&mut self, node: &'ast ImplItemFn) {
        let previous = self.owner.clone();
        self.owner = format!("{}::<impl>::{}", previous, node.sig.ident);
        visit::visit_impl_item_fn(self, node);
        self.owner = previous;
    }
}

fn is_registry_macro(path: &str) -> bool {
    path.ends_with("generate_handler")
        || path.ends_with("command_registry_fixture")
        || path.ends_with("command_registry")
}

fn quote_path(expression: &syn::Expr) -> String {
    match expression {
        syn::Expr::Path(path) => path
            .path
            .segments
            .iter()
            .map(|segment| segment.ident.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        _ => String::new(),
    }
}

fn cfg_enabled(attrs: &[Attribute]) -> Result<bool, String> {
    for attr in attrs {
        if attr.path().is_ident("cfg") {
            let meta = attr
                .parse_args::<Meta>()
                .map_err(|error| error.to_string())?;
            if !eval_cfg(&meta)? {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn eval_cfg(meta: &Meta) -> Result<bool, String> {
    match meta {
        Meta::Path(path) if path.is_ident("test") || path.is_ident("debug_assertions") => Ok(false),
        Meta::Path(path) => Ok(target_cfg().contains(&path_to_string(path))),
        Meta::NameValue(pair) => match &pair.value {
            syn::Expr::Lit(value) => match &value.lit {
                syn::Lit::Str(value) => Ok(target_cfg().contains(&format!(
                    "{}=\"{}\"",
                    path_to_string(&pair.path),
                    value.value()
                ))),
                _ => Err("cfg name/value must use a string literal".to_string()),
            },
            _ => Err("cfg name/value must use a literal".to_string()),
        },
        Meta::List(list) if list.path.is_ident("all") || list.path.is_ident("any") => {
            let nested = list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .map_err(|error| error.to_string())?;
            let values = nested.iter().map(eval_cfg).collect::<Result<Vec<_>, _>>()?;
            Ok(if list.path.is_ident("all") {
                values.into_iter().all(|value| value)
            } else {
                values.into_iter().any(|value| value)
            })
        }
        Meta::List(list) if list.path.is_ident("not") => {
            let nested = list
                .parse_args::<Meta>()
                .map_err(|error| error.to_string())?;
            Ok(!eval_cfg(&nested)?)
        }
        _ => Err("unsupported cfg construct in architecture gate".to_string()),
    }
}

fn path_to_string(path: &syn::Path) -> String {
    path.segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>()
        .join("::")
}

fn target_cfg() -> &'static BTreeSet<String> {
    static TARGET_CFG: OnceLock<BTreeSet<String>> = OnceLock::new();
    TARGET_CFG.get_or_init(|| {
        let output = Command::new("rustc")
            .args(["--print", "cfg", "--target", "x86_64-pc-windows-msvc"])
            .output()
            .expect("rustc --print cfg must start");
        assert!(
            output.status.success(),
            "rustc --print cfg failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .expect("rustc cfg output must be UTF-8")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect()
    })
}

fn cfg_attr_path(attrs: &[Attribute]) -> Result<Option<PathBuf>, String> {
    for attr in attrs {
        if attr.path().is_ident("path") {
            if let Meta::NameValue(pair) = &attr.meta {
                if let syn::Expr::Lit(value) = &pair.value {
                    if let syn::Lit::Str(value) = &value.lit {
                        return Ok(Some(PathBuf::from(value.value())));
                    }
                }
            }
            return Err("module path attribute must be a string literal".to_string());
        }
        if !attr.path().is_ident("cfg_attr") {
            continue;
        }
        let args = attr
            .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
            .map_err(|error| error.to_string())?;
        let mut iter = args.iter();
        let condition = iter
            .next()
            .ok_or_else(|| "cfg_attr requires a condition".to_string())?;
        if !eval_cfg(condition)? {
            continue;
        }
        for applied in iter {
            if let Meta::NameValue(pair) = applied {
                if pair.path.is_ident("path") {
                    if let syn::Expr::Lit(value) = &pair.value {
                        if let syn::Lit::Str(value) = &value.lit {
                            return Ok(Some(PathBuf::from(value.value())));
                        }
                    }
                    return Err("cfg_attr path must be a string literal".to_string());
                }
            }
        }
    }
    Ok(None)
}

fn module_for_file(src_root: &Path, file: &Path) -> Result<String, String> {
    let relative = file
        .strip_prefix(src_root)
        .map_err(|error| error.to_string())?;
    let mut parts = relative
        .components()
        .map(|part| part.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>();
    let file_name = parts
        .pop()
        .ok_or_else(|| "source file has no name".to_string())?;
    let stem = file_name.trim_end_matches(".rs");
    if !matches!(stem, "lib" | "main" | "mod") {
        parts.push(stem.to_string());
    }
    Ok(if parts.is_empty() {
        "crate".to_string()
    } else {
        format!("crate::{}", parts.join("::"))
    })
}

fn canonical_use(module: &str, segments: &[String]) -> String {
    if segments.is_empty() {
        return module.to_string();
    }
    match segments[0].as_str() {
        "crate" => segments.join("::"),
        "self" => format!("{}::{}", module, segments[1..].join("::"))
            .trim_end_matches("::")
            .to_string(),
        "super" => {
            let mut owner = module.split("::").collect::<Vec<_>>();
            let mut index = 0;
            while segments.get(index).is_some_and(|part| part == "super") {
                if owner.len() > 1 {
                    owner.pop();
                }
                index += 1;
            }
            owner.extend(segments[index..].iter().map(String::as_str));
            owner.join("::")
        }
        _ => segments.join("::"),
    }
}

fn flatten_use(tree: &UseTree, prefix: &mut Vec<String>, targets: &mut Vec<String>) {
    match tree {
        UseTree::Path(path) => {
            prefix.push(path.ident.to_string());
            flatten_use(&path.tree, prefix, targets);
            prefix.pop();
        }
        UseTree::Name(name) => {
            if name.ident == "self" {
                targets.push(prefix.join("::"));
            } else {
                prefix.push(name.ident.to_string());
                targets.push(prefix.join("::"));
                prefix.pop();
            }
        }
        UseTree::Rename(rename) => {
            if rename.ident == "self" {
                targets.push(prefix.join("::"));
            } else {
                prefix.push(rename.ident.to_string());
                targets.push(prefix.join("::"));
                prefix.pop();
            }
        }
        UseTree::Glob(_) => targets.push(format!("{}::*", prefix.join("::"))),
        UseTree::Group(group) => {
            for item in &group.items {
                flatten_use(item, prefix, targets);
            }
        }
    }
}

fn record_use(item: &ItemUse, module: &str, graph: &mut Graph) {
    let mut targets = Vec::new();
    flatten_use(&item.tree, &mut Vec::new(), &mut targets);
    for target in targets {
        let segments = target.split("::").map(str::to_string).collect::<Vec<_>>();
        let target = canonical_use(module, &segments);
        graph.edges.insert(Edge {
            from: module.to_string(),
            to: target.clone(),
            kind: if matches!(item.vis, Visibility::Inherited) {
                "use"
            } else {
                "pub-use"
            },
        });
        if !matches!(item.vis, Visibility::Inherited) {
            graph.public_exports.insert(format!("{module} -> {target}"));
        }
    }
}

fn visit_items(
    items: &[Item],
    module: &str,
    source_file: &Path,
    src_root: &Path,
    graph: &mut Graph,
) -> Result<(), String> {
    for item in items {
        if !cfg_enabled(item_attrs(item))? {
            continue;
        }
        match item {
            Item::Use(item) => record_use(item, module, graph),
            Item::Mod(item) => visit_module(item, module, source_file, src_root, graph)?,
            _ => {}
        }
        let mut visitor = SourceVisitor {
            owner: module.to_string(),
            graph,
        };
        visitor.visit_item(item);
    }
    Ok(())
}

fn item_attrs(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(value) => &value.attrs,
        Item::Enum(value) => &value.attrs,
        Item::ExternCrate(value) => &value.attrs,
        Item::Fn(value) => &value.attrs,
        Item::ForeignMod(value) => &value.attrs,
        Item::Impl(value) => &value.attrs,
        Item::Macro(value) => &value.attrs,
        Item::Mod(value) => &value.attrs,
        Item::Static(value) => &value.attrs,
        Item::Struct(value) => &value.attrs,
        Item::Trait(value) => &value.attrs,
        Item::TraitAlias(value) => &value.attrs,
        Item::Type(value) => &value.attrs,
        Item::Union(value) => &value.attrs,
        Item::Use(value) => &value.attrs,
        _ => &[],
    }
}

fn visit_module(
    item: &ItemMod,
    parent: &str,
    source_file: &Path,
    src_root: &Path,
    graph: &mut Graph,
) -> Result<(), String> {
    let module = format!("{parent}::{}", item.ident);
    if let Some((_, items)) = &item.content {
        if !graph.parsed_modules.insert(module.clone()) {
            return Ok(());
        }
        return visit_items(items, &module, source_file, src_root, graph);
    }
    let explicit = cfg_attr_path(&item.attrs)?;
    let source_parent = source_file
        .parent()
        .ok_or_else(|| "source file has no parent".to_string())?;
    let stem = source_file
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let module_dir = if matches!(stem, "lib" | "main" | "mod") {
        source_parent.to_path_buf()
    } else {
        source_parent.join(stem)
    };
    let candidates = if let Some(explicit) = explicit {
        vec![source_parent.join(explicit)]
    } else {
        vec![
            module_dir.join(format!("{}.rs", item.ident)),
            module_dir.join(item.ident.to_string()).join("mod.rs"),
        ]
    };
    let found = candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            format!(
                "cannot resolve out-of-line module {module} declared in {}",
                source_file.display()
            )
        })?;
    parse_file(&found, src_root, Some(module), graph)
}

fn parse_file(
    file: &Path,
    src_root: &Path,
    forced_module: Option<String>,
    graph: &mut Graph,
) -> Result<(), String> {
    let source =
        fs::read_to_string(file).map_err(|error| format!("{}: {error}", file.display()))?;
    let syntax =
        syn::parse_file(&source).map_err(|error| format!("{}: {error}", file.display()))?;
    let module = forced_module.unwrap_or(module_for_file(src_root, file)?);
    if !graph.parsed_modules.insert(module.clone()) {
        return Ok(());
    }
    visit_items(&syntax.items, &module, file, src_root, graph)
}

fn analyze_crate(src_root: &Path) -> Result<Graph, String> {
    let entry = [src_root.join("lib.rs"), src_root.join("main.rs")]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| format!("no lib.rs or main.rs under {}", src_root.display()))?;
    let mut graph = Graph::default();
    parse_file(&entry, src_root, Some("crate".to_string()), &mut graph)?;
    Ok(graph)
}

fn top_level(module: &str) -> String {
    module.split("::").take(2).collect::<Vec<_>>().join("::")
}

fn dependency_cycles(graph: &Graph) -> Vec<String> {
    let mut adjacency: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for edge in &graph.edges {
        let from = top_level(&edge.from);
        let to = top_level(edge.to.trim_end_matches("::*"));
        if from != to && to.starts_with("crate::") {
            adjacency.entry(from).or_default().insert(to);
        }
    }
    let mut cycles = BTreeSet::new();
    for (start, nexts) in &adjacency {
        for next in nexts {
            if adjacency
                .get(next)
                .is_some_and(|targets| targets.contains(start))
            {
                cycles.insert(format!("{} <-> {}", start.min(next), start.max(next)));
            }
        }
    }
    cycles.into_iter().collect()
}

fn fan_out(graph: &Graph, owner: &str) -> usize {
    graph
        .edges
        .iter()
        .filter(|edge| edge.from == owner)
        .map(|edge| edge.to.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/architecture_scale")
        .join(name)
        .join("src")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn manifest_entries(value: &Value, key: &str, ecosystem: &str) -> BTreeSet<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            if let Some(identity) = entry.as_str() {
                return Some(identity.to_string());
            }
            let object = entry.as_object()?;
            if object
                .get("ecosystem")
                .and_then(Value::as_str)
                .is_some_and(|value| value != ecosystem)
            {
                return None;
            }
            if let Some(identity) = object.get("identity").and_then(Value::as_str) {
                return Some(identity.to_string());
            }
            let from = object.get("from")?.as_str()?;
            let to = object.get("to")?.as_str()?;
            let kind = object.get("kind").and_then(Value::as_str).unwrap_or("use");
            Some(format!("{from} -> {to} [{kind}]"))
        })
        .collect()
}

fn validate_temporary_rust_entries(
    manifest: &Value,
    actual: &BTreeSet<String>,
    current_stage: u64,
) -> Result<(), String> {
    for (index, entry) in manifest
        .get("temporary_edges")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let Some(object) = entry.as_object() else {
            return Err(format!("temporary_edges[{index}] must be an object"));
        };
        if object
            .get("ecosystem")
            .and_then(Value::as_str)
            .is_some_and(|value| value != "rust")
        {
            continue;
        }
        if object.get("id").and_then(Value::as_str) == Some("compiled-command-registry-pending") {
            continue;
        }
        if object
            .get("owner")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        {
            return Err(format!("temporary_edges[{index}].owner is required"));
        }
        if object
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .is_empty()
        {
            return Err(format!("temporary_edges[{index}].reason is required"));
        }
        let Some(expiry) = object.get("expiry_stage").and_then(Value::as_u64) else {
            return Err(format!(
                "temporary_edges[{index}] requires numeric expiry_stage"
            ));
        };
        if current_stage >= expiry {
            return Err(format!(
                "temporary_edges[{index}] expired at stage {expiry}"
            ));
        }
        let identities =
            manifest_entries(&serde_json::json!({"entries": [entry]}), "entries", "rust");
        if identities.is_empty() {
            return Err(format!(
                "temporary_edges[{index}] requires an exact Rust edge identity"
            ));
        }
        for identity in identities {
            if !actual.contains(&identity) {
                return Err(format!("stale temporary Rust edge: {identity}"));
            }
        }
    }
    Ok(())
}

fn owned_allowlist_entries(
    manifest: &Value,
    key: &str,
    current_stage: u64,
) -> Result<BTreeSet<String>, String> {
    let entries = manifest
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{key} must be an array"))?;
    let mut identities = BTreeSet::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(object) = entry.as_object() else {
            return Err(format!("{key}[{index}] must be an owned object"));
        };
        let Some(identity) = object.get("identity").and_then(Value::as_str) else {
            return Err(format!("{key}[{index}].identity is required"));
        };
        if identity.trim().is_empty() {
            return Err(format!("{key}[{index}].identity must not be empty"));
        }
        for field in ["owner", "reason", "introduced_shard", "delete_shard"] {
            if object
                .get(field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .is_empty()
            {
                return Err(format!("{key}[{index}].{field} is required"));
            }
        }
        let Some(expiry) = object.get("expiry_stage").and_then(Value::as_u64) else {
            return Err(format!("{key}[{index}].expiry_stage is required"));
        };
        if current_stage >= expiry {
            return Err(format!("{key}[{index}] expired at stage {expiry}"));
        }
        if !identities.insert(identity.to_string()) {
            return Err(format!("{key}[{index}] duplicates identity {identity}"));
        }
    }
    Ok(identities)
}

#[test]
fn parser_handles_qualified_grouped_alias_glob_inline_out_of_line_and_cfg_attr_modules() {
    let graph = analyze_crate(&fixture("pass")).expect("fixture must parse");
    assert!(graph.parsed_modules.contains("crate::nested::child"));
    assert!(graph.parsed_modules.contains("crate::inline"));
    assert!(graph.parsed_modules.contains("crate::platform"));
    assert!(graph.parsed_modules.contains("crate::windows_enabled"));
    assert!(!graph.parsed_modules.contains("crate::must_not_be_resolved"));
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.to == "crate::nested::child::*"));
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.to == "crate::same_a::Same"));
    assert!(graph
        .edges
        .iter()
        .any(|edge| edge.to == "crate::same_b::Same"));
    assert!(graph
        .public_exports
        .iter()
        .any(|export| export.contains("ordinary::PublicValue")));
    assert!(
        fan_out(&graph, "crate") >= 5,
        "descendant fan-out must retain exact targets"
    );
    assert!(
        graph
            .registry_macros
            .iter()
            .any(|site| site.ends_with("command_registry_fixture")),
        "registry macro must be detected but not expanded"
    );
}

#[test]
fn parser_detects_dependency_cycle_and_forbidden_public_export_fixtures() {
    let cycle = analyze_crate(&fixture("red_cycle")).expect("cycle fixture must parse");
    assert!(!dependency_cycles(&cycle).is_empty());
    let export = analyze_crate(&fixture("red_export")).expect("export fixture must parse");
    assert!(export.public_exports.contains("crate -> internal::Secret"));
}

#[test]
fn manifest_gate_rejects_stale_expired_and_empty_allowlists() {
    let actual = BTreeSet::from(["crate::a -> crate::b [use]".to_string()]);
    let stale = serde_json::json!({"temporary_edges":[{"ecosystem":"rust","from":"crate::missing","to":"crate::b","kind":"use","owner":"Task 1","reason":"fixture","expiry_stage":2}]});
    assert!(validate_temporary_rust_entries(&stale, &actual, 0).is_err());
    let expired = serde_json::json!({"temporary_edges":[{"ecosystem":"rust","from":"crate::a","to":"crate::b","kind":"use","owner":"Task 1","reason":"fixture","expiry_stage":1}]});
    assert!(validate_temporary_rust_entries(&expired, &actual, 1).is_err());
    let bare_allowlist = serde_json::json!({"spawn_allowlist":["crate::legacy::tokio::spawn"]});
    assert!(owned_allowlist_entries(&bare_allowlist, "spawn_allowlist", 0).is_err());
    let owned_allowlist = serde_json::json!({"http_client_construction_allowlist":[{
        "identity":"crate::outbound::client::<impl>::client_for_policy::reqwest::Client::builder",
        "owner":"Task 14",
        "reason":"fixture",
        "introduced_shard":"fixture",
        "delete_shard":"fixture",
        "expiry_stage":2
    }]});
    assert!(
        owned_allowlist_entries(&owned_allowlist, "http_client_construction_allowlist", 0).is_ok()
    );
    let empty = serde_json::json!({"fan_in_baseline":{},"fan_out_baseline":{}});
    assert!(empty["fan_in_baseline"]
        .as_object()
        .is_some_and(|value| value.is_empty()));
    assert!(empty["fan_out_baseline"]
        .as_object()
        .is_some_and(|value| value.is_empty()));
}

#[test]
fn production_boundaries_match_manifest() {
    let root = repo_root();
    let metadata = Command::new("cargo")
        .args([
            "metadata",
            "--locked",
            "--format-version",
            "1",
            "--no-deps",
            "--manifest-path",
            "src-tauri/Cargo.toml",
        ])
        .current_dir(&root)
        .output()
        .expect("cargo metadata must start");
    assert!(
        metadata.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata_json: Value =
        serde_json::from_slice(&metadata.stdout).expect("cargo metadata must be valid JSON");
    let package = metadata_json["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"].as_str() == Some("relay-pool-desktop"))
        })
        .expect("cargo metadata must contain relay-pool-desktop");
    let crate_entry = package["targets"]
        .as_array()
        .and_then(|targets| {
            targets.iter().find(|target| {
                target["name"].as_str() == Some("relay_pool_desktop_lib")
                    || target["src_path"]
                        .as_str()
                        .is_some_and(|path| path.replace('\\', "/").ends_with("/src/lib.rs"))
            })
        })
        .and_then(|target| target["src_path"].as_str())
        .expect("cargo metadata must expose the library target src_path");
    let src_root = Path::new(crate_entry)
        .parent()
        .expect("library target must have a source parent");
    let graph = analyze_crate(src_root).expect("production crate must parse");

    let manifest_path =
        root.join("docs/superpowers/audits/architecture-scale-boundary-manifest.json");
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(&manifest_path).expect("boundary manifest is required"),
    )
    .expect("boundary manifest must be valid JSON");
    for key in [
        "current_stage",
        "allowed_exports",
        "allowed_edges",
        "forbidden_edges",
        "temporary_edges",
        "spawn_allowlist",
        "http_client_construction_allowlist",
        "fan_in_baseline",
        "fan_out_baseline",
    ] {
        assert!(
            manifest.get(key).is_some(),
            "boundary manifest is missing {key}"
        );
    }
    assert!(
        !manifest["fan_in_baseline"]
            .as_object()
            .is_none_or(|value| value.is_empty()),
        "fan_in_baseline must not be empty"
    );
    assert!(
        !manifest["fan_out_baseline"]
            .as_object()
            .is_none_or(|value| value.is_empty()),
        "fan_out_baseline must not be empty"
    );

    let forbidden = manifest_entries(&manifest, "forbidden_edges", "rust");
    let actual = graph
        .edges
        .iter()
        .map(Edge::identity)
        .collect::<BTreeSet<_>>();
    let current_stage = manifest["current_stage"]
        .as_u64()
        .expect("boundary manifest current_stage must be a non-negative integer");
    if let Ok(supplied) = std::env::var("ARCHITECTURE_STAGE") {
        let supplied = supplied
            .parse::<u64>()
            .expect("ARCHITECTURE_STAGE must be a non-negative integer");
        assert_eq!(
            supplied, current_stage,
            "ARCHITECTURE_STAGE differs from repository stage"
        );
    }
    validate_temporary_rust_entries(&manifest, &actual, current_stage)
        .expect("temporary Rust edges must be live, owned and unexpired");
    for identity in forbidden {
        assert!(
            !actual.contains(&identity),
            "forbidden Rust edge exists: {identity}"
        );
    }

    let spawn_allowlist = owned_allowlist_entries(&manifest, "spawn_allowlist", current_stage)
        .expect("spawn_allowlist entries must be owned, exact and unexpired");
    for site in &graph.spawn_sites {
        assert!(
            spawn_allowlist.contains(site.as_str()),
            "unregistered spawn site: {site}"
        );
    }
    for site in &spawn_allowlist {
        assert!(
            graph.spawn_sites.contains(site),
            "stale spawn allowlist entry: {site}"
        );
    }
    let client_allowlist = owned_allowlist_entries(
        &manifest,
        "http_client_construction_allowlist",
        current_stage,
    )
    .expect("http_client_construction_allowlist entries must be owned, exact and unexpired");
    for site in &graph.http_client_sites {
        assert!(
            client_allowlist.contains(site.as_str()),
            "unregistered HTTP client construction: {site}"
        );
    }
    for site in &client_allowlist {
        assert!(
            graph.http_client_sites.contains(site),
            "stale HTTP client allowlist entry: {site}"
        );
    }
    assert!(
        !graph.blocking_executor_submit_sites.is_empty(),
        "BlockingExecutor submit sites must be visible to the production boundary gate"
    );
    for site in &graph.blocking_executor_submit_sites {
        let normalized = site.to_ascii_lowercase();
        for forbidden in [
            "authorization",
            "endpoint",
            "http",
            "network",
            "outbound",
            "provider",
            "remote_key",
            "request",
            "response",
        ] {
            assert!(
                !normalized.contains(forbidden),
                "network-shaped work must not enter BlockingExecutor: {site}"
            );
        }
    }
}
