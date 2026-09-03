//! Boundary check: layer edges, sink layers, forbidden imports, forbidden literal classes and
//! the two processing-isolation rules, all decided from `boundary.toml` plus
//! `cargo metadata --all-features` (transitive edges) and a token-level scan of Rust sources.

use cargo_metadata::{CargoOpt, MetadataCommand, Package, PackageId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

#[derive(Debug, Default)]
pub struct Options {
    /// Run only this isolation rule (`capture-path-isolation` | `native-inference-confinement`).
    pub rule: Option<String>,
    /// Run only this check (`forbidden-imports`).
    pub check: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Violation {
    pub id: String,
    pub rule: String,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub workspace: String,
    pub crates_checked: usize,
    pub edges_checked: usize,
    pub scope: String,
    pub violations: Vec<Violation>,
}

#[derive(Debug, Deserialize, Default)]
struct Policy {
    #[serde(default)]
    layers: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    layer_patterns: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    edges: Edges,
    #[serde(default)]
    forbidden_imports: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    literals: Literals,
    #[serde(default)]
    rules: Rules,
    #[serde(default)]
    third_party: ThirdParty,
}

#[derive(Debug, Deserialize, Default)]
struct Edges {
    #[serde(default)]
    restricted: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Deserialize, Default)]
struct Literals {
    #[serde(default)]
    allow_layers: Vec<String>,
    #[serde(default)]
    class_a: ClassA,
    #[serde(default)]
    class_b: ClassB,
}

#[derive(Debug, Deserialize, Default)]
struct ClassA {
    #[serde(default)]
    words: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ClassB {
    #[serde(default)]
    literals: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Rules {
    #[serde(rename = "capture-path-isolation")]
    capture_path_isolation: Option<CapturePathRule>,
    #[serde(rename = "native-inference-confinement")]
    native_inference_confinement: Option<NativeRule>,
}

#[derive(Debug, Deserialize)]
struct CapturePathRule {
    sources: Vec<String>,
    #[serde(default)]
    forbidden: Vec<String>,
    #[serde(default)]
    forbidden_patterns: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct NativeRule {
    #[serde(default)]
    allowed: Vec<String>,
    #[serde(default)]
    allowed_patterns: Vec<String>,
    #[serde(default)]
    native_crates: Vec<String>,
    #[serde(default)]
    build_script_exempt: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct ThirdParty {
    #[serde(default)]
    crates: Vec<String>,
}

pub const CAPTURE_PATH_RULE: &str = "capture-path-isolation";
pub const NATIVE_RULE: &str = "native-inference-confinement";

fn glob_match(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => pattern == name,
    }
}

fn layer_rank(layer: &str) -> Option<u32> {
    layer.strip_prefix('L').and_then(|n| n.parse().ok())
}

impl Policy {
    fn layer_of(&self, name: &str) -> Option<String> {
        for (layer, names) in &self.layers {
            if names.iter().any(|n| n == name) {
                return Some(layer.clone());
            }
        }
        for (layer, patterns) in &self.layer_patterns {
            if patterns.iter().any(|p| glob_match(p, name)) {
                return Some(layer.clone());
            }
        }
        None
    }
}

struct Graph {
    names: HashMap<PackageId, String>,
    members: BTreeSet<PackageId>,
    deps: HashMap<PackageId, Vec<PackageId>>,
    packages: HashMap<PackageId, Package>,
}

impl Graph {
    fn load(root: &Path) -> Result<Self, String> {
        let metadata = MetadataCommand::new()
            .manifest_path(root.join("Cargo.toml"))
            .features(CargoOpt::AllFeatures)
            .exec()
            .map_err(|e| format!("cargo metadata failed: {e}"))?;
        let resolve = metadata
            .resolve
            .ok_or("cargo metadata returned no resolve graph")?;
        let mut deps = HashMap::new();
        for node in resolve.nodes {
            deps.insert(
                node.id.clone(),
                node.deps.iter().map(|d| d.pkg.clone()).collect(),
            );
        }
        let mut names = HashMap::new();
        let mut packages = HashMap::new();
        for package in metadata.packages {
            names.insert(package.id.clone(), package.name.to_string());
            packages.insert(package.id.clone(), package);
        }
        Ok(Self {
            names,
            members: metadata.workspace_members.into_iter().collect(),
            deps,
            packages,
        })
    }

    /// Every package reachable from `start`, with the first dependency path found to it.
    fn reachable(&self, start: &PackageId) -> BTreeMap<PackageId, Vec<String>> {
        let mut seen: BTreeMap<PackageId, Vec<String>> = BTreeMap::new();
        let mut stack = vec![(start.clone(), vec![self.names[start].clone()])];
        while let Some((id, path)) = stack.pop() {
            for dep in self.deps.get(&id).into_iter().flatten() {
                if seen.contains_key(dep) || dep == start {
                    continue;
                }
                let mut next = path.clone();
                next.push(self.names[dep].clone());
                seen.insert(dep.clone(), next.clone());
                stack.push((dep.clone(), next));
            }
        }
        seen
    }
}

pub fn check(root: &Path, opts: &Options) -> Result<Report, String> {
    let policy_text = std::fs::read_to_string(root.join("boundary.toml"))
        .map_err(|e| format!("cannot read boundary.toml under {}: {e}", root.display()))?;
    let policy: Policy =
        toml::from_str(&policy_text).map_err(|e| format!("boundary.toml is invalid: {e}"))?;
    let graph = Graph::load(root)?;
    let full = opts.rule.is_none() && opts.check.is_none();
    let scope = match (&opts.rule, &opts.check) {
        (Some(rule), _) => format!("rule:{rule}"),
        (_, Some(check)) => format!("check:{check}"),
        _ => "full".to_string(),
    };
    if let Some(rule) = &opts.rule {
        if rule != CAPTURE_PATH_RULE && rule != NATIVE_RULE {
            return Err(format!(
                "unknown rule {rule}; declared rules are {CAPTURE_PATH_RULE} and {NATIVE_RULE}"
            ));
        }
    }
    let mut violations = Vec::new();
    let mut edges_checked = 0usize;
    let third_party: BTreeSet<&str> = policy
        .third_party
        .crates
        .iter()
        .map(String::as_str)
        .collect();

    let mut reach: BTreeMap<PackageId, BTreeMap<PackageId, Vec<String>>> = BTreeMap::new();
    for member in &graph.members {
        reach.insert(member.clone(), graph.reachable(member));
    }

    if full {
        for member in &graph.members {
            let name = &graph.names[member];
            let Some(layer) = policy.layer_of(name) else {
                if !third_party.contains(name.as_str()) {
                    violations.push(Violation {
                        id: format!("unlisted-crate:{name}"),
                        rule: "layers".into(),
                        detail: format!(
                            "workspace crate {name} is not assigned to a layer in boundary.toml"
                        ),
                    });
                }
                continue;
            };
            let rank =
                layer_rank(&layer).ok_or_else(|| format!("layer {layer} has no numeric rank"))?;
            for (dep, path) in &reach[member] {
                if !graph.members.contains(dep) {
                    continue;
                }
                let dep_name = &graph.names[dep];
                let Some(dep_layer) = policy.layer_of(dep_name) else {
                    continue;
                };
                edges_checked += 1;
                let dep_rank = layer_rank(&dep_layer)
                    .ok_or_else(|| format!("layer {dep_layer} has no numeric rank"))?;
                let top = policy
                    .layers
                    .keys()
                    .filter_map(|l| layer_rank(l))
                    .max()
                    .unwrap_or(0);
                if rank == top {
                    continue; // composition roots may depend on anything
                }
                let allowed = match policy.edges.restricted.get(&layer) {
                    Some(list) => list.iter().any(|l| l == &dep_layer),
                    None => dep_rank < rank,
                };
                let sink_reached =
                    policy.edges.restricted.contains_key(&dep_layer) && dep_rank >= rank;
                if !allowed || sink_reached {
                    violations.push(Violation {
                        id: format!("edge:{name}->{dep_name}"),
                        rule: "layers".into(),
                        detail: format!(
                            "{layer} crate {name} reaches {dep_layer} crate {dep_name} via {}",
                            path.join(" -> ")
                        ),
                    });
                }
            }
        }
    }

    if full || opts.check.as_deref() == Some("forbidden-imports") {
        for member in &graph.members {
            let name = &graph.names[member];
            let Some(forbidden) = policy.forbidden_imports.get(name) else {
                continue;
            };
            let dir = crate_dir(&graph.packages[member]);
            for (rel, text) in rust_sources(&dir) {
                for (line, path) in paths_in(&text) {
                    if forbidden
                        .iter()
                        .any(|f| path == *f || path.starts_with(&format!("{f}::")))
                    {
                        violations.push(Violation {
                            id: format!("import:{name}:{rel}:{line}"),
                            rule: "forbidden-imports".into(),
                            detail: format!("{name} uses forbidden path {path} at {rel}:{line}"),
                        });
                    }
                }
            }
        }
    }

    if full {
        let words: BTreeSet<String> = policy
            .literals
            .class_a
            .words
            .iter()
            .map(|w| w.to_lowercase())
            .collect();
        let lits: BTreeSet<String> = policy
            .literals
            .class_b
            .literals
            .iter()
            .map(|l| l.to_lowercase())
            .collect();
        for member in &graph.members {
            let name = &graph.names[member];
            if third_party.contains(name.as_str()) {
                continue;
            }
            if let Some(layer) = policy.layer_of(name) {
                if policy.literals.allow_layers.iter().any(|l| l == &layer) {
                    continue;
                }
            }
            let dir = crate_dir(&graph.packages[member]);
            for (rel, text) in rust_sources(&dir) {
                for token in tokenize(&text) {
                    match token.kind {
                        TokenKind::Ident => {
                            if split_words(&token.text).iter().any(|w| words.contains(w)) {
                                violations.push(Violation {
                                    id: format!("literal-a:{name}:{rel}:{}", token.line),
                                    rule: "literals-class-a".into(),
                                    detail: format!("identifier `{}` at {rel}:{} contains a service identifier word", token.text, token.line),
                                });
                            }
                        }
                        TokenKind::Str => {
                            if lits.contains(&token.text.to_lowercase()) {
                                violations.push(Violation {
                                    id: format!("literal-b:{name}:{rel}:{}", token.line),
                                    rule: "literals-class-b".into(),
                                    detail: format!("string literal at {rel}:{} equals a declared process, package or host literal", token.line),
                                });
                            }
                        }
                        TokenKind::Other => {}
                    }
                }
            }
        }
    }

    if full || opts.rule.as_deref() == Some(CAPTURE_PATH_RULE) {
        if let Some(rule) = &policy.rules.capture_path_isolation {
            for member in &graph.members {
                let name = &graph.names[member];
                if !rule.sources.iter().any(|s| s == name) {
                    continue;
                }
                for (dep, path) in &reach[member] {
                    let dep_name = &graph.names[dep];
                    edges_checked += 1;
                    let forbidden = rule.forbidden.iter().any(|f| f == dep_name)
                        || rule
                            .forbidden_patterns
                            .iter()
                            .any(|p| glob_match(p, dep_name));
                    if forbidden {
                        violations.push(Violation {
                            id: format!("capture-path:{name}->{dep_name}"),
                            rule: CAPTURE_PATH_RULE.into(),
                            detail: format!(
                                "capture-path crate {name} reaches {dep_name} via {}",
                                path.join(" -> ")
                            ),
                        });
                    }
                }
            }
        }
    }

    if full || opts.rule.as_deref() == Some(NATIVE_RULE) {
        if let Some(rule) = &policy.rules.native_inference_confinement {
            // Declared native inference bindings may be reached only by the processor host and
            // the processor adapters it loads. Additionally, an undeclared crate that both declares
            // `links` and compiles C/C++ in its build script (a build-dependency on cc, cmake,
            // cxx-build, bindgen or pkg-config) must not reach the capture path at all. `links`
            // alone is also used for pure-Rust version coordination (wasm-bindgen-shared), which is
            // not native linkage.
            const NATIVE_BUILD_TOOLS: [&str; 5] =
                ["cc", "cmake", "cxx-build", "bindgen", "pkg-config"];
            let declared: BTreeSet<PackageId> = graph
                .packages
                .iter()
                .filter(|(_, p)| rule.native_crates.iter().any(|n| n == &p.name.to_string()))
                .map(|(id, _)| id.clone())
                .collect();
            let build_script_native: BTreeSet<PackageId> = graph
                .packages
                .iter()
                .filter(|(_, p)| {
                    let name = p.name.to_string();
                    let compiles_native = p.dependencies.iter().any(|d| {
                        d.kind == cargo_metadata::DependencyKind::Build
                            && NATIVE_BUILD_TOOLS.contains(&d.name.as_str())
                    });
                    p.links.is_some()
                        && compiles_native
                        && !rule.build_script_exempt.iter().any(|n| n == &name)
                        && !declared.contains(&p.id)
                })
                .map(|(id, _)| id.clone())
                .collect();
            let capture_path: Vec<String> = policy
                .rules
                .capture_path_isolation
                .as_ref()
                .map(|r| r.sources.clone())
                .unwrap_or_default();
            for member in &graph.members {
                let name = &graph.names[member];
                let allowed = rule.allowed.iter().any(|a| a == name)
                    || rule.allowed_patterns.iter().any(|p| glob_match(p, name));
                if allowed {
                    continue;
                }
                let on_capture_path = capture_path.iter().any(|c| c == name);
                for (dep, path) in &reach[member] {
                    edges_checked += 1;
                    let dep_name = &graph.names[dep];
                    if declared.contains(dep)
                        || (on_capture_path && build_script_native.contains(dep))
                    {
                        violations.push(Violation {
                            id: format!("native-link:{name}->{dep_name}"),
                            rule: NATIVE_RULE.into(),
                            detail: format!(
                                "{name} reaches native-linking crate {dep_name} via {}",
                                path.join(" -> ")
                            ),
                        });
                    }
                }
            }
        }
    }

    violations.sort_by(|a, b| a.id.cmp(&b.id));
    violations.dedup_by(|a, b| a.id == b.id);
    Ok(Report {
        workspace: root.display().to_string(),
        crates_checked: graph.members.len(),
        edges_checked,
        scope,
        violations,
    })
}

pub fn print_text(report: &Report) {
    println!(
        "boundary check ({}) on {}: {} crates, {} edges checked",
        report.scope, report.workspace, report.crates_checked, report.edges_checked
    );
    for v in &report.violations {
        println!("VIOLATION {} [{}] {}", v.id, v.rule, v.detail);
    }
    if report.violations.is_empty() {
        println!("OK: no boundary violations");
    } else {
        println!("{} violation(s)", report.violations.len());
    }
}

fn crate_dir(package: &Package) -> PathBuf {
    package
        .manifest_path
        .parent()
        .map(|p| p.as_std_path().to_path_buf())
        .unwrap_or_default()
}

fn rust_sources(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                // Skip build output and nested packages/workspaces (fixtures, examples with their
                // own manifest): nested workspace members are scanned as crates in their own right.
                if path.file_name().is_some_and(|n| n == "target")
                    || path.join("Cargo.toml").is_file()
                {
                    continue;
                }
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    let rel = path
                        .strip_prefix(dir)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((rel, text));
                }
            }
        }
    }
    out.sort();
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Ident,
    Str,
    Other,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub text: String,
    pub line: usize,
}

/// A small Rust lexer that distinguishes identifiers, string literals and everything else,
/// and drops comments (line, block, doc) so that they are never scanned.
pub fn tokenize(src: &str) -> Vec<Token> {
    let chars: Vec<char> = src.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let mut line = 1;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            line += 1;
            i += 1;
            continue;
        }
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let mut depth = 0;
            loop {
                if i >= chars.len() {
                    break;
                }
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    i += 2;
                    continue;
                }
                if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth -= 1;
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                    continue;
                }
                if chars[i] == '\n' {
                    line += 1;
                }
                i += 1;
            }
            continue;
        }
        // string literals: "..", b"..", c"..", r".."/r#".."#, br".."/br#".."#
        let mut prefix = 0;
        if c == 'b' || c == 'c' {
            prefix = 1;
        }
        let raw = chars.get(i + prefix) == Some(&'r')
            && matches!(chars.get(i + prefix + 1), Some('"') | Some('#'));
        let plain = chars.get(i + prefix) == Some(&'"');
        if (c == '"')
            || (prefix == 1 && plain)
            || raw
            || (c == 'r' && matches!(chars.get(i + 1), Some('"') | Some('#')))
        {
            let start_line = line;
            let mut j = i + prefix;
            let is_raw = chars.get(j) == Some(&'r');
            if is_raw {
                j += 1;
                let mut hashes = 0;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if chars.get(j) == Some(&'"') {
                    j += 1;
                    let mut text = String::new();
                    while j < chars.len() {
                        if chars[j] == '"' {
                            let mut k = 0;
                            while k < hashes && chars.get(j + 1 + k) == Some(&'#') {
                                k += 1;
                            }
                            if k == hashes {
                                j += 1 + hashes;
                                break;
                            }
                        }
                        if chars[j] == '\n' {
                            line += 1;
                        }
                        text.push(chars[j]);
                        j += 1;
                    }
                    tokens.push(Token {
                        kind: TokenKind::Str,
                        text,
                        line: start_line,
                    });
                    i = j;
                    continue;
                }
                // `r#ident` raw identifier: fall through to identifier lexing below
            } else {
                j += 1; // opening quote
                let mut text = String::new();
                while j < chars.len() && chars[j] != '"' {
                    if chars[j] == '\\' {
                        text.push(chars[j]);
                        j += 1;
                        if j < chars.len() {
                            if chars[j] == '\n' {
                                line += 1;
                            }
                            text.push(chars[j]);
                            j += 1;
                        }
                        continue;
                    }
                    if chars[j] == '\n' {
                        line += 1;
                    }
                    text.push(chars[j]);
                    j += 1;
                }
                j += 1; // closing quote
                tokens.push(Token {
                    kind: TokenKind::Str,
                    text,
                    line: start_line,
                });
                i = j;
                continue;
            }
        }
        if c == '\'' {
            // char literal or lifetime
            if chars.get(i + 1) == Some(&'\\') {
                let mut j = i + 2;
                while j < chars.len() && chars[j] != '\'' {
                    j += 1;
                }
                i = j + 1;
                continue;
            }
            if chars.get(i + 2) == Some(&'\'') {
                i += 3;
                continue;
            }
            i += 1; // lifetime: the identifier that follows is lexed as an ordinary identifier
            continue;
        }
        if c.is_alphabetic() || c == '_' {
            let mut j = i;
            while j < chars.len() && (chars[j].is_alphanumeric() || chars[j] == '_') {
                j += 1;
            }
            let text: String = chars[i..j].iter().collect();
            tokens.push(Token {
                kind: TokenKind::Ident,
                text,
                line,
            });
            i = j;
            continue;
        }
        if c.is_ascii_digit() {
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_alphanumeric() || chars[j] == '_' || chars[j] == '.')
            {
                j += 1;
            }
            i = j;
            continue;
        }
        if c == ':' && chars.get(i + 1) == Some(&':') {
            tokens.push(Token {
                kind: TokenKind::Other,
                text: "::".into(),
                line,
            });
            i += 2;
            continue;
        }
        tokens.push(Token {
            kind: TokenKind::Other,
            text: c.to_string(),
            line,
        });
        i += 1;
    }
    tokens
}

/// Split an identifier on `_` and CamelCase boundaries into lowercase words.
pub fn split_words(ident: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = ident.chars().collect();
    for (idx, &ch) in chars.iter().enumerate() {
        if ch == '_' {
            if !current.is_empty() {
                words.push(current.to_lowercase());
                current.clear();
            }
            continue;
        }
        let boundary = ch.is_uppercase()
            && idx > 0
            && (chars[idx - 1].is_lowercase()
                || chars[idx - 1].is_ascii_digit()
                || (chars[idx - 1].is_uppercase()
                    && chars.get(idx + 1).is_some_and(|n| n.is_lowercase())));
        if boundary && !current.is_empty() {
            words.push(current.to_lowercase());
            current.clear();
        }
        current.push(ch);
    }
    if !current.is_empty() {
        words.push(current.to_lowercase());
    }
    words
}

/// Every `a::b::c` path in the source, including each leaf of `use a::{b, c::{d}}` groups.
pub fn paths_in(src: &str) -> Vec<(usize, String)> {
    let tokens = tokenize(src);
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        if tokens[i].kind == TokenKind::Ident {
            let start = i;
            let (paths, next) = collect_paths(&tokens, start, String::new());
            for (line, path) in paths {
                if path.contains("::") {
                    out.push((line, path));
                }
            }
            i = next.max(start + 1);
            continue;
        }
        i += 1;
    }
    out
}

fn collect_paths(tokens: &[Token], mut i: usize, prefix: String) -> (Vec<(usize, String)>, usize) {
    let mut out = Vec::new();
    let mut path = prefix;
    let mut line = tokens.get(i).map(|t| t.line).unwrap_or(0);
    while i < tokens.len() {
        let t = &tokens[i];
        match t.kind {
            TokenKind::Ident => {
                if path.is_empty() {
                    line = t.line;
                    path = t.text.clone();
                } else if path.ends_with("::") {
                    path.push_str(&t.text);
                } else {
                    break;
                }
                i += 1;
            }
            TokenKind::Other if t.text == "::" => {
                if path.is_empty() {
                    break;
                }
                path.push_str("::");
                i += 1;
                if tokens
                    .get(i)
                    .is_some_and(|n| n.kind == TokenKind::Other && n.text == "{")
                {
                    let group_prefix = path.clone();
                    i += 1;
                    loop {
                        match tokens.get(i) {
                            None => break,
                            Some(n) if n.kind == TokenKind::Other && n.text == "}" => {
                                i += 1;
                                break;
                            }
                            Some(n) if n.kind == TokenKind::Other && n.text == "," => {
                                i += 1;
                            }
                            Some(n) if n.kind == TokenKind::Ident => {
                                let (inner, next) = collect_paths(tokens, i, group_prefix.clone());
                                out.extend(inner);
                                i = next.max(i + 1);
                            }
                            Some(_) => {
                                i += 1;
                            }
                        }
                    }
                    return (out, i);
                }
            }
            _ => break,
        }
    }
    if path.ends_with("::") {
        path.truncate(path.len() - 2);
    }
    if !path.is_empty() {
        out.push((line, path));
    }
    (out, i)
}
