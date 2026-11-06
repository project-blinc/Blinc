#![allow(clippy::unnecessary_unwrap, clippy::ptr_arg)]
//! Route definition and path matching
//!
//! Routes are defined with Express-style path patterns and matched
//! via a trie for O(depth) lookup.

use rustc_hash::FxHashMap;

use crate::RouteView;

/// Route parameters extracted from matched path segments
#[derive(Clone, Debug, Default)]
pub struct RouteParams(pub FxHashMap<String, String>);

impl RouteParams {
    pub fn new() -> Self {
        Self(FxHashMap::default())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    pub fn get_parsed<T: std::str::FromStr>(&self, key: &str) -> Option<T> {
        self.0.get(key).and_then(|s| s.parse().ok())
    }

    pub fn insert(&mut self, key: String, value: String) {
        self.0.insert(key, value);
    }
}

/// Query parameters from the URL
#[derive(Clone, Debug, Default)]
pub struct QueryParams(pub FxHashMap<String, String>);

impl QueryParams {
    pub fn new() -> Self {
        Self(FxHashMap::default())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(|s| s.as_str())
    }

    pub fn parse(query: &str) -> Self {
        let mut params = FxHashMap::default();
        for pair in query.split('&') {
            if let Some((key, value)) = pair.split_once('=') {
                params.insert(key.to_string(), value.to_string());
            }
        }
        Self(params)
    }
}

/// Context passed to route view builders
pub struct RouteContext {
    pub params: RouteParams,
    pub query: QueryParams,
    pub path: String,
    pub router: crate::Router,
}

/// A route definition
pub struct Route {
    pub path: String,
    pub name: Option<String>,
    pub view: Option<RouteView>,
    pub children: Vec<Route>,
    pub guards: Vec<crate::NavigationGuard>,
    pub transition: Option<crate::transition::PageTransition>,
}

impl Route {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            name: None,
            view: None,
            children: Vec::new(),
            guards: Vec::new(),
            transition: None,
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn view(mut self, view: RouteView) -> Self {
        self.view = Some(view);
        self
    }

    pub fn child(mut self, child: Route) -> Self {
        self.children.push(child);
        self
    }

    pub fn transition(mut self, transition: crate::transition::PageTransition) -> Self {
        self.transition = Some(transition);
        self
    }

    pub fn guard(mut self, guard: crate::NavigationGuard) -> Self {
        self.guards.push(guard);
        self
    }
}

/// A matched route with extracted parameters
#[derive(Clone, Debug)]
pub struct MatchedRoute {
    pub path: String,
    pub name: Option<String>,
    pub params: RouteParams,
    pub query: QueryParams,
    pub view_index: usize,
    pub transition: Option<crate::transition::PageTransition>,
}

/// Segment type in the route trie
#[derive(Clone, Debug)]
enum SegmentType {
    Static(String),
    Param(String),
    Wildcard(String),
}

/// Trie node for route matching
struct TrieNode {
    segment: SegmentType,
    children: Vec<TrieNode>,
    route_index: Option<usize>,
    route_name: Option<String>,
    route_transition: Option<crate::transition::PageTransition>,
}

/// Route trie for O(depth) path matching
pub struct RouteTrie {
    roots: Vec<TrieNode>,
    not_found_index: Option<usize>,
}

impl Default for RouteTrie {
    fn default() -> Self {
        Self::new()
    }
}

impl RouteTrie {
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            not_found_index: None,
        }
    }

    /// Add a route to the trie
    pub fn add(
        &mut self,
        path: &str,
        route_index: usize,
        name: Option<&str>,
        transition: Option<crate::transition::PageTransition>,
    ) {
        if path == "/" {
            self.add_root(route_index, name, transition);
            return;
        }
        let segments = parse_segments(path);
        let mut current_children = &mut self.roots;

        for seg in &segments {
            let pos = current_children
                .iter()
                .position(|n| segment_matches_type(&n.segment, seg));

            if let Some(pos) = pos {
                current_children = &mut current_children[pos].children;
            } else {
                let node = TrieNode {
                    segment: seg.clone(),
                    children: Vec::new(),
                    route_index: None,
                    route_name: None,
                    route_transition: None,
                };
                current_children.push(node);
                let last = current_children.len() - 1;
                current_children = &mut current_children[last].children;
            }
        }

        // Mark the terminal node
        // Walk back to the last node we created/found
        let terminal = find_terminal_mut(&mut self.roots, &segments);
        if let Some(node) = terminal {
            node.route_index = Some(route_index);
            node.route_name = name.map(|s| s.to_string());
            node.route_transition = transition;
        }
    }

    /// Set the not-found route index
    pub fn set_not_found(&mut self, index: usize) {
        self.not_found_index = Some(index);
    }

    fn add_root(
        &mut self,
        route_index: usize,
        name: Option<&str>,
        transition: Option<crate::transition::PageTransition>,
    ) {
        let node = TrieNode {
            segment: SegmentType::Static(String::new()),
            children: Vec::new(),
            route_index: Some(route_index),
            route_name: name.map(|s| s.to_string()),
            route_transition: transition,
        };
        self.roots.push(node);
    }

    /// Match a path against the trie
    pub fn match_path(&self, path: &str) -> Option<MatchedRoute> {
        let (path_part, query_str) = path.split_once('?').unwrap_or((path, ""));
        let query = QueryParams::parse(query_str);
        let segments: Vec<&str> = path_part.split('/').filter(|s| !s.is_empty()).collect();

        // Root "/" match: empty segments
        if segments.is_empty() {
            for node in &self.roots {
                if let SegmentType::Static(ref s) = node.segment
                    && s.is_empty()
                    && node.route_index.is_some()
                {
                    return Some(MatchedRoute {
                        path: "/".to_string(),
                        name: node.route_name.clone(),
                        params: RouteParams::new(),
                        query,
                        view_index: node.route_index.unwrap(),
                        transition: node.route_transition.clone(),
                    });
                }
            }
        }

        let mut params = RouteParams::new();
        if let Some(matched) = match_recursive(&self.roots, &segments, 0, &mut params) {
            Some(MatchedRoute {
                path: path_part.to_string(),
                name: matched.1,
                params,
                query,
                view_index: matched.0,
                transition: matched.2,
            })
        } else {
            self.not_found_index.map(|nf_idx| MatchedRoute {
                path: path_part.to_string(),
                name: Some("not_found".to_string()),
                params: RouteParams::new(),
                query,
                view_index: nf_idx,
                transition: None,
            })
        }
    }
}

fn parse_segments(path: &str) -> Vec<SegmentType> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            if let Some(name) = s.strip_prefix(':') {
                SegmentType::Param(name.to_string())
            } else if let Some(name) = s.strip_prefix('*') {
                SegmentType::Wildcard(name.to_string())
            } else {
                SegmentType::Static(s.to_string())
            }
        })
        .collect()
}

fn segment_matches_type(existing: &SegmentType, new: &SegmentType) -> bool {
    matches!(
        (existing, new),
        (SegmentType::Static(a), SegmentType::Static(b)) if a == b
    ) || matches!(
        (existing, new),
        (SegmentType::Param(a), SegmentType::Param(b)) if a == b
    ) || matches!(
        (existing, new),
        (SegmentType::Wildcard(a), SegmentType::Wildcard(b)) if a == b
    )
}

fn find_terminal_mut<'a>(
    nodes: &'a mut Vec<TrieNode>,
    segments: &[SegmentType],
) -> Option<&'a mut TrieNode> {
    if segments.is_empty() {
        return None;
    }

    let seg = &segments[0];
    for node in nodes.iter_mut() {
        if segment_matches_type(&node.segment, seg) {
            if segments.len() == 1 {
                return Some(node);
            }
            return find_terminal_mut(&mut node.children, &segments[1..]);
        }
    }
    None
}

fn match_recursive(
    nodes: &[TrieNode],
    segments: &[&str],
    depth: usize,
    params: &mut RouteParams,
) -> Option<(
    usize,
    Option<String>,
    Option<crate::transition::PageTransition>,
)> {
    if depth >= segments.len() {
        return None;
    }

    let segment = segments[depth];

    // Priority: static > param > wildcard
    // 1. Try static match
    for node in nodes {
        if let SegmentType::Static(ref s) = node.segment
            && s == segment
        {
            if depth + 1 == segments.len() && node.route_index.is_some() {
                return Some((
                    node.route_index.unwrap(),
                    node.route_name.clone(),
                    node.route_transition.clone(),
                ));
            }
            if let Some(result) = match_recursive(&node.children, segments, depth + 1, params) {
                return Some(result);
            }
        }
    }

    // 2. Try param match
    for node in nodes {
        if let SegmentType::Param(ref name) = node.segment {
            params.insert(name.clone(), segment.to_string());
            if depth + 1 == segments.len() && node.route_index.is_some() {
                return Some((
                    node.route_index.unwrap(),
                    node.route_name.clone(),
                    node.route_transition.clone(),
                ));
            }
            if let Some(result) = match_recursive(&node.children, segments, depth + 1, params) {
                return Some(result);
            }
            // Backtrack
            params.0.remove(name);
        }
    }

    // 3. Try wildcard match
    for node in nodes {
        if let SegmentType::Wildcard(ref name) = node.segment {
            let rest = segments[depth..].join("/");
            params.insert(name.clone(), rest);
            if node.route_index.is_some() {
                return Some((
                    node.route_index.unwrap(),
                    node.route_name.clone(),
                    node.route_transition.clone(),
                ));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_route() {
        let mut trie = RouteTrie::new();
        trie.add("/about", 0, Some("about"), None);
        trie.add("/users", 1, Some("users"), None);

        let m = trie.match_path("/about").unwrap();
        assert_eq!(m.view_index, 0);
        assert_eq!(m.name.as_deref(), Some("about"));

        let m = trie.match_path("/users").unwrap();
        assert_eq!(m.view_index, 1);

        assert!(trie.match_path("/missing").is_none());
    }

    #[test]
    fn test_param_route() {
        let mut trie = RouteTrie::new();
        trie.add("/users/:id", 0, None, None);

        let m = trie.match_path("/users/42").unwrap();
        assert_eq!(m.view_index, 0);
        assert_eq!(m.params.get("id"), Some("42"));
    }

    #[test]
    fn test_nested_route() {
        let mut trie = RouteTrie::new();
        trie.add("/users/:id/posts", 0, None, None);

        let m = trie.match_path("/users/42/posts").unwrap();
        assert_eq!(m.view_index, 0);
        assert_eq!(m.params.get("id"), Some("42"));
    }

    #[test]
    fn test_query_params() {
        let mut trie = RouteTrie::new();
        trie.add("/search", 0, None, None);

        let m = trie.match_path("/search?q=hello&page=2").unwrap();
        assert_eq!(m.view_index, 0);
        assert_eq!(m.query.get("q"), Some("hello"));
        assert_eq!(m.query.get("page"), Some("2"));
    }

    #[test]
    fn test_not_found() {
        let mut trie = RouteTrie::new();
        trie.add("/home", 0, None, None);
        trie.set_not_found(99);

        let m = trie.match_path("/missing").unwrap();
        assert_eq!(m.view_index, 99);
    }
}
