//! Bounded resource pool for reuse across concurrent captures. See `Plans.md`
//! (task 7.1).
//!
//! [`Pool`] caps the number of simultaneously-leased resources and reuses idle
//! ones instead of creating fresh — the mechanism behind "concurrent captures
//! reuse a bounded pool". It is generic and thread-safe (an internal `Mutex`),
//! so it can hold the browser/tab handles the render path will pool; this is
//! the pure pooling layer, independent of any live browser.

use std::sync::Mutex;

/// A bounded pool of reusable resources of type `T`.
pub struct Pool<T> {
    inner: Mutex<Inner<T>>,
    capacity: usize,
}

struct Inner<T> {
    /// Returned-but-still-alive resources, available for reuse.
    idle: Vec<T>,
    /// How many resources are currently checked out.
    leased: usize,
}

impl<T> Pool<T> {
    /// Create a pool that allows at most `capacity` simultaneously-leased
    /// resources. `capacity` is clamped to at least 1.
    pub fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(Inner {
                idle: Vec::new(),
                leased: 0,
            }),
            capacity: capacity.max(1),
        }
    }

    /// The configured maximum number of concurrent leases.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Acquire a resource: reuse an idle one if available, otherwise create a
    /// fresh one via `make` when below capacity. Returns `None` when the pool is
    /// exhausted (all `capacity` resources are leased and none are idle).
    ///
    /// `make` runs without the pool lock held (so it may be slow, e.g. spawning
    /// a browser) after the slot is reserved.
    pub fn acquire(&self, make: impl FnOnce() -> T) -> Option<T> {
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(item) = inner.idle.pop() {
                inner.leased += 1;
                return Some(item);
            }
            if inner.leased >= self.capacity {
                return None;
            }
            inner.leased += 1; // reserve the slot before the (possibly slow) make
        }
        Some(make())
    }

    /// Like [`acquire`](Self::acquire) but with a *fallible* factory: reuse an
    /// idle resource, else create one via `make` (when below capacity),
    /// propagating its error and freeing the reserved slot on failure. `Ok(None)`
    /// means the pool is exhausted.
    pub fn acquire_with<E>(&self, make: impl FnOnce() -> Result<T, E>) -> Result<Option<T>, E> {
        {
            let mut inner = self.inner.lock().unwrap();
            if let Some(item) = inner.idle.pop() {
                inner.leased += 1;
                return Ok(Some(item));
            }
            if inner.leased >= self.capacity {
                return Ok(None);
            }
            inner.leased += 1; // reserve the slot before the (possibly slow) make
        }
        match make() {
            Ok(item) => Ok(Some(item)),
            Err(e) => {
                self.discard(); // creation failed → release the reserved slot
                Err(e)
            }
        }
    }

    /// Free a leased slot *without* returning a resource — e.g. when a leased
    /// resource has died and must not be reused.
    pub fn discard(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.leased = inner.leased.saturating_sub(1);
    }

    /// Return a resource to the pool for reuse, freeing one lease slot.
    pub fn release(&self, item: T) {
        let mut inner = self.inner.lock().unwrap();
        inner.idle.push(item);
        inner.leased = inner.leased.saturating_sub(1);
    }

    /// Number of idle (reusable) resources currently held.
    pub fn idle_count(&self) -> usize {
        self.inner.lock().unwrap().idle.len()
    }

    /// Number of resources currently leased out.
    pub fn leased_count(&self) -> usize {
        self.inner.lock().unwrap().leased
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn capacity_is_at_least_one() {
        assert_eq!(Pool::<()>::new(0).capacity(), 1);
        assert_eq!(Pool::<()>::new(5).capacity(), 5);
    }

    #[test]
    fn reuses_an_idle_resource_instead_of_creating() {
        let created = Cell::new(0);
        let pool = Pool::new(2);

        let first = pool
            .acquire(|| {
                created.set(created.get() + 1);
                created.get()
            })
            .unwrap();
        assert_eq!(created.get(), 1);
        pool.release(first);

        // The idle resource is reused; `make` is not called again.
        let reused = pool
            .acquire(|| {
                created.set(created.get() + 1);
                created.get()
            })
            .unwrap();
        assert_eq!(created.get(), 1, "an idle resource should be reused");
        assert_eq!(reused, 1);
        assert_eq!(pool.leased_count(), 1);
    }

    #[test]
    fn bounds_concurrent_leases_at_capacity() {
        let pool = Pool::new(2);
        let _a = pool.acquire(|| 1).unwrap();
        let _b = pool.acquire(|| 2).unwrap();
        assert!(pool.acquire(|| 3).is_none(), "third lease exceeds capacity");
        assert_eq!(pool.leased_count(), 2);
        assert_eq!(pool.idle_count(), 0);
    }

    #[test]
    fn acquire_with_propagates_factory_error_and_frees_the_slot() {
        let pool: Pool<u32> = Pool::new(2);
        let err: Result<Option<u32>, &str> = pool.acquire_with(|| Err("boom"));
        assert_eq!(err, Err("boom"));
        // The reserved slot was freed, so we can still acquire up to capacity.
        assert_eq!(pool.leased_count(), 0);
        assert!(pool.acquire_with(|| Ok::<_, &str>(1)).unwrap().is_some());
        assert!(pool.acquire_with(|| Ok::<_, &str>(2)).unwrap().is_some());
        assert!(pool.acquire_with(|| Ok::<_, &str>(3)).unwrap().is_none()); // exhausted
    }

    #[test]
    fn discard_frees_a_slot_without_returning_the_item() {
        let pool: Pool<u32> = Pool::new(1);
        let _item = pool.acquire(|| 1).unwrap();
        assert_eq!(pool.leased_count(), 1);
        // The leased resource "died" — discard frees the slot without pooling it.
        pool.discard();
        assert_eq!(pool.leased_count(), 0);
        assert_eq!(pool.idle_count(), 0, "discarded item is not pooled");
        assert!(
            pool.acquire(|| 2).is_some(),
            "slot freed for a fresh resource"
        );
    }

    #[test]
    fn releasing_frees_a_slot_for_reacquire() {
        let pool = Pool::new(1);
        let a = pool.acquire(|| 1).unwrap();
        assert!(pool.acquire(|| 2).is_none()); // at capacity
        pool.release(a);
        assert!(
            pool.acquire(|| 3).is_some(),
            "a released slot can be reacquired"
        );
    }
}
