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
}
