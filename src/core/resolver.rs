//! Cross-file symbol resolver.
//!
//! Builds a lookup table over all parsed [`SourceFile`]s and uses the
//! import graph to find the *correct* definition when the same name appears
//! in multiple files.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::core::types::{FunctionDef, SourceFile};

pub struct ResolverIndex {
    /// `"file::name"` → definition
    exact:   HashMap<String, FunctionDef>,
    /// `"name"` → [(file, def)]
    by_name: HashMap<String, Vec<(String, FunctionDef)>>,
    /// import graph: file → set of files it imports
    imports: HashMap<String, HashSet<String>>,
}

impl ResolverIndex {
    pub fn build(files: &[SourceFile]) -> Self {
        let mut exact:   HashMap<String, FunctionDef>             = HashMap::new();
        let mut by_name: HashMap<String, Vec<(String, FunctionDef)>> = HashMap::new();
        let mut imports: HashMap<String, HashSet<String>>          = HashMap::new();

        for file in files {
            for def in &file.functions {
                let key = format!("{}::{}", file.path, def.name);
                exact.insert(key, def.clone());
                by_name.entry(def.name.clone())
                       .or_default()
                       .push((file.path.clone(), def.clone()));
            }
            imports.insert(
                file.path.clone(),
                file.imports.iter().cloned().collect(),
            );
        }

        Self { exact, by_name, imports }
    }

    // ── Lookups ──────────────────────────────────────────────────────────────

    pub fn get_exact(&self, file: &str, name: &str) -> Option<&FunctionDef> {
        self.exact.get(&format!("{}::{}", file, name))
    }

    /// Resolve a callee name relative to a call-site file.
    ///
    /// Priority:
    /// 1. Same file
    /// 2. Transitively imported files
    /// 3. Any file (with ambiguity warning)
    pub fn resolve_callee(
        &self,
        name:        &str,
        caller_file: &str,
        warnings:    &mut Vec<String>,
    ) -> Option<FunctionDef> {
        let candidates = self.by_name.get(name)?;

        if let Some((_, def)) = candidates.iter().find(|(f, _)| f == caller_file) {
            return Some(def.clone());
        }

        let reachable = self.reachable_imports(caller_file);
        if let Some((_, def)) = candidates.iter().find(|(f, _)| reachable.contains(f.as_str())) {
            return Some(def.clone());
        }

        match candidates.len() {
            0 => None,
            1 => Some(candidates[0].1.clone()),
            _ => {
                warnings.push(format!(
                    "ambiguous `{}` (in: {}); using first match",
                    name,
                    candidates.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>().join(", ")
                ));
                Some(candidates[0].1.clone())
            }
        }
    }

    /// Resolve a modifier / decorator name.
    pub fn resolve_modifier(
        &self,
        name:        &str,
        caller_file: &str,
        warnings:    &mut Vec<String>,
    ) -> Option<FunctionDef> {
        let all: Vec<_> = self.by_name.get(name)?
            .iter()
            .filter(|(_, d)| d.is_modifier)
            .cloned()
            .collect();

        if all.is_empty() { return None; }

        if let Some((_, def)) = all.iter().find(|(f, _)| f == caller_file) {
            return Some(def.clone());
        }

        let reachable = self.reachable_imports(caller_file);
        if let Some((_, def)) = all.iter().find(|(f, _)| reachable.contains(f.as_str())) {
            return Some(def.clone());
        }

        if all.len() > 1 {
            warnings.push(format!(
                "ambiguous modifier `{}` (in: {}); using first",
                name,
                all.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>().join(", ")
            ));
        }
        Some(all[0].1.clone())
    }

    pub fn all_definitions(&self) -> &HashMap<String, FunctionDef> { &self.exact }

    // ── Import graph BFS ─────────────────────────────────────────────────────

    fn reachable_imports<'a>(&'a self, start: &'a str) -> HashSet<&'a str> {
        let mut visited: HashSet<&str> = HashSet::new();
        let mut queue:   VecDeque<&str> = VecDeque::new();
        queue.push_back(start);
        while let Some(file) = queue.pop_front() {
            if !visited.insert(file) { continue; }
            if let Some(imps) = self.imports.get(file) {
                for imp in imps { queue.push_back(imp.as_str()); }
            }
        }
        visited
    }
}
