//-
// Copyright 2021 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Strategies used for abstract state machine testing.

use crate::bits::{BitSetLike, VarBitSet};
use crate::collection::SizeRange;
use crate::num::sample_uniform_incl;
use crate::std_facade::fmt::{Debug, Formatter, Result};
use crate::std_facade::Vec;
use crate::strategy::{
    traits::{NewTree, ValueTree},
    Strategy,
};
use crate::test_runner::TestRunner;
use core::cell::Cell;

/// TODO
pub trait AbstractStateMachine {
    /// TODO
    type State: Clone;
    /// TODO
    type Transition: Clone + Debug;
    /// TODO
    type StateStrategy: Strategy<Value = Self::State>;
    /// TODO
    type TransitionStrategy: Strategy<Value = Self::Transition>;

    /// TODO
    fn init_state() -> Self::StateStrategy;
    /// TODO
    fn preconditions(
        state: &Self::State,
        transition: &Self::Transition,
    ) -> bool;
    /// TODO
    fn transitions(state: &Self::State) -> Self::TransitionStrategy;
    /// TODO
    fn next(state: Self::State, transition: &Self::Transition) -> Self::State;

    /// TODO
    fn sequential_strategy(
        size: impl Into<SizeRange>,
    ) -> Sequential<
        Self::State,
        Self::Transition,
        Self::StateStrategy,
        Self::TransitionStrategy,
    > {
        Sequential {
            size: size.into(),
            init_state: Self::init_state,
            preconditions: Self::preconditions,
            transitions: Self::transitions,
            next: Self::next,
        }
    }
}

/// A helper to declare the associated types for `AbstractStateMachine`.
///
/// Note that the use `impl Strategy` type alias currently requires the nightly
/// feature `#![feature(type_alias_impl_trait)]` (rust stable 1.49.0).
#[macro_export]
macro_rules! state_and_transition_type {
    { $state:ty, $transition:ty } => {
        type State = $state;
        type Transition = $transition;
        type StateStrategy = impl Strategy<Value = $state>;
        type TransitionStrategy = impl Strategy<Value = $transition>;
    }
}

/// In a sequential state machine strategy, we first generate an acceptable
/// sequence of transitions. That is a sequence that satisfy the given
/// pre-conditions. The acceptability of each transition in the sequence only
/// depends on the current state of the state machine, which is updated by the
/// transitions with the `next` function.
///
/// Then we iteratively try to delete transitions from the back of the list,
/// until we can do so no further.
///
/// After that, we again iteratively attempt to shrink the individual
/// transitions, but this time starting from the front of the list.
///
/// For `complicate()`, we simply undo the last shrink operation, if
/// there was any.
pub struct Sequential<
    State: Clone,
    // Debug required by Strategy::Value
    Transition: Clone + Debug,
    StateStrategy: Strategy<Value = State>,
    TransitionStrategy: Strategy<Value = Transition>,
> {
    size: SizeRange,
    init_state: fn() -> StateStrategy,
    preconditions: fn(state: &State, transition: &Transition) -> bool,
    transitions: fn(state: &State) -> TransitionStrategy,
    next: fn(state: State, transition: &Transition) -> State,
}

impl<
        State: Clone,
        Transition: Clone + Debug,
        StateStrategy: Strategy<Value = State>,
        TransitionStrategy: Strategy<Value = Transition>,
    > Debug
    for Sequential<State, Transition, StateStrategy, TransitionStrategy>
{
    fn fmt(&self, f: &mut Formatter) -> Result {
        f.debug_struct("Sequential")
            /*             .field("state", &self.state)
            .field("transitions", &self.transitions)
            .field("machine", &"<elided for clarity>")
            .field("trace", &self.trace) */
            .finish()
    }
}

impl<
        State: Clone,
        Transition: Clone + Debug,
        StateStrategy: Strategy<Value = State>,
        TransitionStrategy: Strategy<Value = Transition>,
    > Strategy
    for Sequential<State, Transition, StateStrategy, TransitionStrategy>
{
    type Tree =
        SequentialValueTree<State, Transition, TransitionStrategy::Tree>;
    type Value = Vec<TransitionStrategy::Value>;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        let state_tree = (self.init_state)().new_tree(runner)?;
        let (start, end) = self.size.start_end_incl();
        let max_size = sample_uniform_incl(runner, start, end);
        let mut transitions = Vec::with_capacity(max_size);
        let mut acceptable_transitions = Vec::with_capacity(max_size);
        let mut state = state_tree.current();
        let initial_state = state.clone();
        while transitions.len() < max_size {
            let transition_tree =
                (self.transitions)(&state).new_tree(runner)?;
            let transition = transition_tree.current();
            if (self.preconditions)(&state, &transition) {
                transitions.push(transition_tree);
                state = (self.next)(state, &transition);
                acceptable_transitions
                    .push((Cell::new(TransitionState::Current), transition));
            } else {
                runner.reject_local("Pre-conditions were not satisfied")?;
            }
        }
        let max_ix = max_size - 1;
        Ok(SequentialValueTree {
            initial_state,
            preconditions: self.preconditions,
            next: self.next,
            transitions,
            included_transitions: VarBitSet::saturated(max_size),
            shrinkable_transitions: VarBitSet::saturated(max_size),
            acceptable_transitions,
            min_size: start,
            max_ix,
            shrink: Shrink::DeleteTransition(max_ix),
            prev_shrink: None,
        })
    }
}

#[derive(Clone, Copy, Debug)]
enum Shrink {
    DeleteTransition(usize),
    ShrinkTransition(usize),
}
use Shrink::*;

#[derive(Clone, Copy, Debug)]
enum TransitionState {
    /// The transition that is equal to the result of `ValueTree::current()`
    Current,
    /// The transition has been simplified, but rejected by pre-conditions
    SimplifyRejected,
    /// The transition has been complicated, but rejected by pre-conditions
    ComplicateRejected,
}
use TransitionState::*;

/// The generated value tree for a sequential state machine.
pub struct SequentialValueTree<
    State: Clone,
    Transition: Clone + Debug,
    TransitionValueTree: ValueTree<Value = Transition>,
> {
    initial_state: State,
    preconditions: fn(&State, &Transition) -> bool,
    next: fn(State, &Transition) -> State,
    transitions: Vec<TransitionValueTree>,
    included_transitions: VarBitSet,
    shrinkable_transitions: VarBitSet,
    /// The sequence of included transitions that satisfy the pre-conditions
    acceptable_transitions: Vec<(Cell<TransitionState>, Transition)>,
    min_size: usize,
    max_ix: usize,
    shrink: Shrink,
    prev_shrink: Option<Shrink>,
}

impl<
        State: Clone,
        Transition: Clone + Debug,
        TransitionValueTree: ValueTree<Value = Transition>,
    > SequentialValueTree<State, Transition, TransitionValueTree>
{
    /// The current included acceptable transitions. When `ix` is not `None`,
    /// the transition at this index is taken from its current value, instead of
    /// its acceptable value.
    fn current_at(&self, ix: Option<usize>) -> Vec<Transition> {
        self.acceptable_transitions
            .iter()
            .enumerate()
            .filter(|&(this_ix, _)| self.included_transitions.test(this_ix))
            .map(|(this_ix, (_, transition))| match ix {
                Some(ix) if this_ix == ix => self.transitions[ix].current(),
                _ => transition.clone(),
            })
            .collect()
    }

    /// Find if all the simplifications and complications of the included
    /// transitions have been rejected.
    fn all_rejected(&mut self) -> bool {
        self.acceptable_transitions
            .iter()
            .enumerate()
            .filter(|&(ix, _)| self.included_transitions.test(ix))
            .all(|(_, (state, _transition))| match state.get() {
                SimplifyRejected | ComplicateRejected => true,
                _ => false,
            })
    }

    /// Try to apply the next `self.shrink`.
    fn try_simplify(&mut self) -> bool {
        if let DeleteTransition(ix) = self.shrink {
            if self.included_transitions.count() == self.min_size {
                // Can't delete any more transitions, move on to shrinking
                self.shrink = ShrinkTransition(0);
            } else {
                self.included_transitions.clear(ix);
                self.prev_shrink = Some(self.shrink);
                self.shrink = if ix == 0 {
                    // Reached the beginning of the list, move on to
                    // shrinking
                    ShrinkTransition(0)
                } else {
                    // Try to delete the previous transition next
                    DeleteTransition(ix - 1)
                };
                // If this delete is not acceptable, undo it and try again
                if !self.check_acceptable(None) {
                    self.included_transitions.set(ix);
                    self.prev_shrink = None;
                    return self.try_simplify();
                }
                self.shrinkable_transitions.clear(ix);
                return true;
            }
        }

        while let ShrinkTransition(ix) = self.shrink {
            if self.shrinkable_transitions.count() == 0 {
                // Nothing more we can do
                println!("EXIT no more shrink transitions, len {}, ix {}, shrinkable {}", self.transitions.len(), ix, self.shrinkable_transitions.count());
                return false;
            }

            if !self.included_transitions.test(ix) {
                // No use shrinking something we're not including
                self.shrink = self.next_shrink_transition(ix);
                continue;
            }

            if let SimplifyRejected = self.acceptable_transitions[ix].0.get() {
                // This transition is already simplified and rejected
                self.shrink = self.next_shrink_transition(ix);
            } else if self.transitions[ix].simplify() {
                self.prev_shrink = Some(self.shrink);
                if self.check_acceptable(Some(ix)) {
                    self.acceptable_transitions[ix] =
                        (Cell::new(Current), self.transitions[ix].current());
                    return true;
                } else {
                    self.acceptable_transitions[ix].0.set(SimplifyRejected);
                    self.shrink = self.next_shrink_transition(ix);
                    return self.simplify();
                }
            } else {
                self.shrinkable_transitions.clear(ix);
                self.shrink = self.next_shrink_transition(ix);
            }
        }

        panic!("Unexpected shrink state");
    }

    /// Find the next shrink transition. Loops back to the front of the list
    /// when the end is reached.
    fn next_shrink_transition(&mut self, current_ix: usize) -> Shrink {
        if current_ix == self.max_ix {
            // Either loop back to the start of the list...
            ShrinkTransition(0)
        } else {
            // ...or move on to the next transition
            ShrinkTransition(current_ix + 1)
        }
    }

    /// Check if the sequence of included acceptable transitions is acceptable.
    ///  When `ix` is not `None`, the transition at the given index is taken
    /// from its current value.
    fn check_acceptable(&mut self, ix: Option<usize>) -> bool {
        let transitions = self.current_at(ix);
        let mut state = self.initial_state.clone();
        for transition in transitions.iter() {
            let current_acceptable = (&self.preconditions)(&state, transition);
            if current_acceptable {
                state = (&self.next)(state, transition);
            } else {
                return false;
            }
        }
        true
    }

    /// Find if there's any acceptable included transition that is not current,
    /// starting from the given index. Expects that all the included transitions
    /// are rejected (when `all_rejected` returns `true`).
    fn try_to_find_acceptable(&mut self, ix: usize) -> bool {
        let mut ix_to_check = ix;
        loop {
            if self.included_transitions.test(ix_to_check)
                && self.check_acceptable(Some(ix_to_check))
            {
                self.acceptable_transitions[ix_to_check] = (
                    Cell::new(Current),
                    self.transitions[ix_to_check].current(),
                );
                return true;
            }
            // Move on to the next transition
            if ix_to_check == self.max_ix {
                ix_to_check = 0;
            } else {
                ix_to_check = ix_to_check + 1;
            }
            // We're back to where we started, there nothing left to do
            if ix_to_check == ix {
                return false;
            }
        }
    }
}

impl<
        State: Clone,
        Transition: Clone + Debug,
        TransitionValueTree: ValueTree<Value = Transition>,
    > ValueTree
    for SequentialValueTree<State, Transition, TransitionValueTree>
{
    type Value = Vec<Transition>;

    fn current(&self) -> Self::Value {
        // The current included acceptable transitions
        self.current_at(None)
    }

    fn simplify(&mut self) -> bool {
        if self.all_rejected() {
            if let Some(ShrinkTransition(ix)) = self.prev_shrink {
                return self.try_to_find_acceptable(ix);
            }
            false
        } else {
            self.try_simplify()
        }
    }

    fn complicate(&mut self) -> bool {
        match self.prev_shrink {
            None => false,
            Some(DeleteTransition(ix)) => {
                // Undo the last item we deleted. Can't complicate any further,
                // so unset prev_shrink.
                self.included_transitions.set(ix);
                self.shrinkable_transitions.set(ix);
                self.prev_shrink = None;
                true
            }
            Some(ShrinkTransition(ix)) => {
                if self.transitions[ix].complicate() {
                    if self.check_acceptable(Some(ix)) {
                        self.acceptable_transitions[ix] = (
                            Cell::new(Current),
                            self.transitions[ix].current(),
                        );
                        // Don't unset prev_shrink; we may be able to complicate
                        // again.
                        return true;
                    } else {
                        self.acceptable_transitions[ix]
                            .0
                            .set(ComplicateRejected);
                    }
                }
                // Can't complicate the last element any further.
                self.prev_shrink = None;
                false
            }
        }
    }
}