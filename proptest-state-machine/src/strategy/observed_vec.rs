//-
// Copyright 2023 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use std::{
    fmt::{Debug, Formatter, Result},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use proptest::std_facade::Vec;

/// A wrapper around a Vec<T> that keeps track of how many items has been yielded.
///
/// Used as in the [`proptest::strategy::ValueTree`] impl for
/// [`super::SequentialValueTree`] to communicate back which transitions were not
/// seen by the test runner and thus are safe to delete.
#[derive(Clone, Default)]
pub struct ObservedVec<T> {
    seen_counter: Arc<AtomicUsize>,
    transitions: Vec<T>,
}

pub struct IntoIter<T> {
    seen_counter: Arc<AtomicUsize>,
    transitions: std::vec::IntoIter<T>,
}

impl<T> IntoIterator for ObservedVec<T> {
    type Item = T;

    type IntoIter = IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        IntoIter {
            seen_counter: self.seen_counter,
            transitions: self.transitions.into_iter(),
        }
    }
}

impl<T> Iterator for IntoIter<T> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.transitions.next();

        if next.is_some() {
            self.seen_counter.fetch_add(1, Ordering::SeqCst);
        }

        next
    }
}

impl<T: Debug> Debug for ObservedVec<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        self.transitions.fmt(f)
    }
}

impl<T> ObservedVec<T> {
    /// Returns a new [`ObservedVec`].
    pub(super) fn new(
        seen_counter: Arc<AtomicUsize>,
        transitions: Vec<T>,
    ) -> Self {
        Self {
            seen_counter,
            transitions,
        }
    }

    /// Returns the number of transitions
    pub fn len(&self) -> usize {
        self.transitions.len()
    }

    /// Returns true if the number of transitions is 0
    pub fn is_empty(&self) -> bool {
        self.transitions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use proptest::{num, prelude::*};

    proptest! {
        #[test]
        fn test_fmt(
            stuff in prop::collection::vec(num::i32::ANY, 1..100),
        ) {
            test_fmt_aux(stuff);
        }
    }

    fn test_fmt_aux(vec: Vec<i32>) {
        let transitions = ObservedVec {
            seen_counter: Default::default(),
            transitions: vec.clone().into_iter().rev().collect(),
        };

        assert_eq!(format!("{transitions:?}",), format!("{vec:?}",));
    }

    proptest! {
        #[test]
        fn test_iter(
            stuff in prop::collection::vec(num::i32::ANY, 1..100),
        ) {
            test_iter_aux(stuff);
        }
    }

    fn test_iter_aux(vec: Vec<i32>) {
        let seen_counter = Default::default();
        let transitions = ObservedVec {
            seen_counter: Arc::clone(&seen_counter),
            transitions: vec.clone().into_iter().rev().collect(),
        };

        let len = vec.len();

        for (v, t) in vec.into_iter().zip(transitions) {
            assert_eq!(v, t)
        }

        assert_eq!(len, seen_counter.load(std::sync::atomic::Ordering::SeqCst));
    }
}
