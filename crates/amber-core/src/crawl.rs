//! Crawl scope and frontier: the pure bookkeeping for a bounded multi-page
//! crawl — link-following, scope restriction, visited de-duplication, and
//! depth/page budgets. See `Plans.md` (task 3.1).
//!
//! This module is I/O-free. A crawl driver fetches each URL handed out by
//! [`Frontier::next_url`], extracts links (e.g. via [`crate::meta`]), and feeds them
//! back through [`Frontier::add_links`]; the frontier decides what is in scope,
//! unseen, and within budget.

use std::collections::{HashSet, VecDeque};

use url::Url;

use crate::{http, meta};

/// Which URLs a crawl is allowed to follow.
#[derive(Debug, Clone)]
pub struct CrawlScope {
    /// Lowercased host the crawl is anchored to.
    host: String,
    /// Also follow subdomains of `host`.
    include_subdomains: bool,
    /// Restrict to URLs whose path starts with this prefix.
    path_prefix: Option<String>,
}

impl CrawlScope {
    /// Scope a crawl to the seed URL's exact host.
    pub fn same_host(seed: &Url) -> Self {
        Self {
            host: seed.host_str().unwrap_or_default().to_ascii_lowercase(),
            include_subdomains: false,
            path_prefix: None,
        }
    }

    /// Also follow subdomains of the anchor host.
    pub fn including_subdomains(mut self, yes: bool) -> Self {
        self.include_subdomains = yes;
        self
    }

    /// Restrict the crawl to URLs under `prefix` (a path prefix like `/docs/`).
    pub fn under_path(mut self, prefix: impl Into<String>) -> Self {
        self.path_prefix = Some(prefix.into());
        self
    }

    /// Whether `url` may be crawled under this scope.
    pub fn in_scope(&self, url: &Url) -> bool {
        if !matches!(url.scheme(), "http" | "https") {
            return false;
        }
        let Some(host) = url.host_str().map(|h| h.to_ascii_lowercase()) else {
            return false;
        };
        let host_ok = if self.include_subdomains {
            host == self.host || host.ends_with(&format!(".{}", self.host))
        } else {
            host == self.host
        };
        if !host_ok {
            return false;
        }
        match &self.path_prefix {
            Some(prefix) => url.path().starts_with(prefix.as_str()),
            None => true,
        }
    }
}

/// Bounds on a crawl: how many pages and how deep.
#[derive(Debug, Clone, Copy)]
pub struct CrawlLimits {
    /// Maximum number of pages handed out by [`Frontier::next_url`].
    pub max_pages: usize,
    /// Maximum link depth from the seed (seed is depth 0).
    pub max_depth: usize,
}

impl Default for CrawlLimits {
    fn default() -> Self {
        Self {
            max_pages: 100,
            max_depth: 3,
        }
    }
}

/// A breadth-first crawl frontier with scope, de-duplication, and budgets.
#[derive(Debug)]
pub struct Frontier {
    scope: CrawlScope,
    limits: CrawlLimits,
    queue: VecDeque<(Url, usize)>,
    seen: HashSet<String>,
    dispatched: usize,
}

impl Frontier {
    /// Create a frontier seeded with `seed` at depth 0.
    pub fn new(seed: Url, scope: CrawlScope, limits: CrawlLimits) -> Self {
        let mut f = Self {
            scope,
            limits,
            queue: VecDeque::new(),
            seen: HashSet::new(),
            dispatched: 0,
        };
        f.enqueue(seed, 0);
        f
    }

    /// Enqueue `url` at `depth`. Returns `true` if it was added — i.e. it is
    /// within the depth budget, in scope, and not already seen.
    pub fn enqueue(&mut self, url: Url, depth: usize) -> bool {
        if depth > self.limits.max_depth || !self.scope.in_scope(&url) {
            return false;
        }
        if !self.seen.insert(normalize(&url)) {
            return false;
        }
        self.queue.push_back((url, depth));
        true
    }

    /// Enqueue discovered `links` as children of a page at `parent_depth`,
    /// returning how many were newly added.
    pub fn add_links<I>(&mut self, parent_depth: usize, links: I) -> usize
    where
        I: IntoIterator<Item = Url>,
    {
        links
            .into_iter()
            .filter(|u| self.enqueue(u.clone(), parent_depth + 1))
            .count()
    }

    /// Hand out the next URL to crawl (with its depth), or `None` once the page
    /// budget is exhausted or the queue is empty.
    pub fn next_url(&mut self) -> Option<(Url, usize)> {
        if self.dispatched >= self.limits.max_pages {
            return None;
        }
        let item = self.queue.pop_front()?;
        self.dispatched += 1;
        Some(item)
    }

    /// Number of pages handed out so far.
    pub fn dispatched(&self) -> usize {
        self.dispatched
    }
}

/// Run a bounded breadth-first crawl from `seed`, using `fetch_links` to fetch
/// each page and return the links discovered on it. Returns the crawled URLs in
/// visit order, respecting scope, depth, the page budget, and de-duplication.
///
/// I/O-agnostic: `fetch_links` does whatever fetching/extraction the caller
/// wants (HTTP fetch + link extraction, a browser render, …). A page that
/// fails to fetch simply contributes no links but still counts as visited.
pub fn crawl_with<F>(seed: Url, scope: CrawlScope, limits: CrawlLimits, mut fetch_links: F) -> Vec<Url>
where
    F: FnMut(&Url) -> Vec<Url>,
{
    let mut frontier = Frontier::new(seed, scope, limits);
    let mut visited = Vec::new();
    while let Some((url, depth)) = frontier.next_url() {
        let links = fetch_links(&url);
        frontier.add_links(depth, links);
        visited.push(url);
    }
    visited
}

/// Run a bounded crawl over HTTP: fetch each page via [`crate::http`] and follow
/// the absolute links found in its HTML (via [`crate::meta`]). Non-HTML or
/// failed responses contribute no links. Returns the crawled URLs in visit
/// order. See [`crawl_with`] for the scope/depth/budget semantics.
pub fn crawl(seed: Url, scope: CrawlScope, limits: CrawlLimits) -> Vec<Url> {
    crawl_with(seed, scope, limits, |url| {
        match http::fetch(url) {
            Ok(page) if http::content_type_is_html(page.content_type.as_deref()) => {
                meta::extract(&page.html, &page.final_url)
                    .links
                    .iter()
                    .filter_map(|l| Url::parse(l).ok())
                    .collect()
            }
            _ => Vec::new(),
        }
    })
}

/// Normalize a URL for de-duplication: drop the fragment (scheme/host are
/// already lowercased by the URL parser; paths stay case-sensitive).
fn normalize(url: &Url) -> String {
    let mut u = url.clone();
    u.set_fragment(None);
    u.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn same_host_scope_excludes_other_hosts_and_schemes() {
        let scope = CrawlScope::same_host(&url("https://example.com/"));
        assert!(scope.in_scope(&url("https://example.com/page")));
        assert!(!scope.in_scope(&url("https://other.com/page")));
        // Subdomains excluded by default.
        assert!(!scope.in_scope(&url("https://blog.example.com/page")));
        // Non-web schemes excluded.
        assert!(!scope.in_scope(&url("ftp://example.com/x")));
    }

    #[test]
    fn subdomains_and_path_prefix_scope() {
        let scope = CrawlScope::same_host(&url("https://example.com/"))
            .including_subdomains(true)
            .under_path("/docs/");
        assert!(scope.in_scope(&url("https://example.com/docs/intro")));
        assert!(scope.in_scope(&url("https://api.example.com/docs/v1")));
        // Right host/subdomain but wrong path.
        assert!(!scope.in_scope(&url("https://example.com/blog/x")));
        // ".example.com" suffix must be a real subdomain, not a look-alike.
        assert!(!scope.in_scope(&url("https://notexample.com/docs/x")));
    }

    #[test]
    fn frontier_seeds_and_dedupes() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let mut f = Frontier::new(seed.clone(), scope, CrawlLimits::default());
        // Seed is queued; re-enqueuing it (or a fragment variant) is a no-op.
        assert!(!f.enqueue(seed.clone(), 0));
        assert!(!f.enqueue(url("https://example.com/#section"), 0));
        let (first, depth) = f.next_url().unwrap();
        assert_eq!(first, seed);
        assert_eq!(depth, 0);
    }

    #[test]
    fn add_links_enqueues_in_scope_children_at_next_depth() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let mut f = Frontier::new(seed, scope, CrawlLimits::default());
        let (_seed, d0) = f.next_url().unwrap();
        let added = f.add_links(
            d0,
            [
                url("https://example.com/a"),
                url("https://other.com/b"), // out of scope
                url("https://example.com/a#frag"), // dup of /a
                url("https://example.com/c"),
            ],
        );
        assert_eq!(added, 2, "only the two unique in-scope links");
        let (_a, d1) = f.next_url().unwrap();
        assert_eq!(d1, 1, "children are one level deeper than the seed");
    }

    #[test]
    fn depth_budget_is_enforced() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let limits = CrawlLimits {
            max_pages: 100,
            max_depth: 1,
        };
        let mut f = Frontier::new(seed, scope, limits);
        assert!(f.enqueue(url("https://example.com/a"), 1)); // at the limit
        assert!(!f.enqueue(url("https://example.com/b"), 2)); // beyond it
    }

    #[test]
    fn page_budget_caps_dispatch() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let limits = CrawlLimits {
            max_pages: 2,
            max_depth: 5,
        };
        let mut f = Frontier::new(seed, scope, limits);
        f.enqueue(url("https://example.com/a"), 1);
        f.enqueue(url("https://example.com/b"), 1);
        assert!(f.next_url().is_some()); // seed
        assert!(f.next_url().is_some()); // /a
        assert!(f.next_url().is_none(), "page budget of 2 reached");
        assert_eq!(f.dispatched(), 2);
    }

    // ---- crawl_with (driver, mock fetcher) -------------------------------

    fn paths(urls: &[Url]) -> Vec<&str> {
        urls.iter().map(|u| u.path()).collect()
    }

    #[test]
    fn crawl_with_visits_in_scope_graph_breadth_first() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let fetch = |u: &Url| match u.path() {
            "/" => vec![url("https://example.com/a"), url("https://example.com/b")],
            "/a" => vec![url("https://example.com/c"), url("https://other.com/x")],
            "/b" => vec![url("https://example.com/a")], // duplicate
            _ => vec![],
        };
        let visited = crawl_with(seed, scope, CrawlLimits::default(), fetch);
        // BFS order; the external link and the duplicate are skipped.
        assert_eq!(paths(&visited), vec!["/", "/a", "/b", "/c"]);
    }

    #[test]
    fn crawl_with_respects_page_budget() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let fetch = |u: &Url| match u.path() {
            "/" => vec![url("https://example.com/a"), url("https://example.com/b")],
            _ => vec![],
        };
        let limits = CrawlLimits {
            max_pages: 2,
            max_depth: 5,
        };
        let visited = crawl_with(seed, scope, limits, fetch);
        assert_eq!(visited.len(), 2);
    }

    #[test]
    fn crawl_with_respects_depth_budget() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let fetch = |u: &Url| match u.path() {
            "/" => vec![url("https://example.com/a")],
            "/a" => vec![url("https://example.com/b")], // would be depth 2
            _ => vec![],
        };
        let limits = CrawlLimits {
            max_pages: 100,
            max_depth: 1,
        };
        let visited = crawl_with(seed, scope, limits, fetch);
        assert_eq!(paths(&visited), vec!["/", "/a"]);
    }

    #[test]
    #[ignore = "performs real network requests; run with --ignored"]
    fn crawl_example_com_stays_in_scope() {
        let seed = url("https://example.com/");
        let scope = CrawlScope::same_host(&seed);
        let visited = crawl(seed, scope, CrawlLimits::default());
        // example.com links only to iana.org (out of scope), so just the seed.
        assert_eq!(visited.len(), 1);
        assert_eq!(visited[0].path(), "/");
    }
}
