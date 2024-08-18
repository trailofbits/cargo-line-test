use std::{
    cmp::{max, min},
    collections::BTreeSet,
    ops::{Add, Range},
};

#[derive(Clone, Eq, PartialEq)]
struct DisjointRange<T>(Range<T>);

impl<T: std::fmt::Debug> std::fmt::Debug for DisjointRange<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

// smoelius: Ordering `DisjointRange`s by `end` makes it easier to implement `RangeSet::contains`
// and `RangeSet::remove`.
impl<T: Ord> Ord for DisjointRange<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.end.cmp(&other.0.end)
    }
}

impl<T: Ord> PartialOrd for DisjointRange<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Default, Debug)]
pub struct RangeSet<T>(BTreeSet<DisjointRange<T>>);

#[allow(private_bounds)]
impl<T: Add<Output = T> + Clone + One + Ord> RangeSet<T> {
    pub fn insert_range(&mut self, mut value: Range<T>) {
        let mut new_range_set = BTreeSet::new();

        for range in &self.0 {
            if unionable(&value, &range.0) {
                value = union(value, range.0.clone());
            } else {
                new_range_set.insert(range.clone());
            }
        }

        debug_assert!(!new_range_set
            .iter()
            .any(|range| unionable(&value, &range.0)));

        new_range_set.insert(DisjointRange(value));

        self.0 = new_range_set;
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn contains(&self, value: T) -> bool {
        self.find_disjoint_range(&value).is_some()
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn remove(&mut self, value: T) -> bool {
        let Some(disjoint_range) = self.find_disjoint_range(&value) else {
            return false;
        };

        self.0.remove(&disjoint_range);

        let value_succ = value.clone() + T::one();

        if disjoint_range.0.start < value {
            self.0.insert(DisjointRange(disjoint_range.0.start..value));
        }

        if value_succ < disjoint_range.0.end {
            self.0
                .insert(DisjointRange(value_succ..disjoint_range.0.end));
        }

        true
    }

    fn find_disjoint_range(&self, value: &T) -> Option<DisjointRange<T>> {
        let value_succ = value.clone() + T::one();
        let singleton = DisjointRange(value.clone()..value_succ.clone());
        let mut disjoint_range_iter = self.0.range(singleton.clone()..=singleton);
        let disjoint_range = disjoint_range_iter.next().cloned()?;

        // smoelius: `disjoint_range_iter` should match at most one disjoint range.
        debug_assert!(disjoint_range_iter.next().is_none());

        Some(disjoint_range)
    }
}

#[cfg_attr(dylint_lib = "supplementary", allow(commented_code))]
fn unionable<T: Add<Output = T> + Clone + One + Ord>(x: &Range<T>, y: &Range<T>) -> bool {
    if x.start <= y.start {
        x.end >= y.start
    } else {
        // y.start < x.start
        y.end >= x.start
    }
}

fn union<T: Clone + Ord>(x: Range<T>, y: Range<T>) -> Range<T> {
    min(x.start, y.start)..max(x.end, y.end)
}

impl<T> IntoIterator for RangeSet<T> {
    type Item = Range<T>;
    // smoelius: Use of `Vec` here is ugly.
    type IntoIter = <Vec<Range<T>> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter {
        self.0
            .into_iter()
            .map(|disjoint_range| disjoint_range.0)
            .collect::<Vec<_>>()
            .into_iter()
    }
}

trait One {
    fn one() -> Self;
}

impl One for u32 {
    fn one() -> Self {
        1
    }
}
