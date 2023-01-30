//-
// Copyright 2021 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

#[macro_use]
extern crate proptest;

use std::cmp;
use std::collections::BinaryHeap;

/// A hand-rolled implementation of a binary heap, like
/// https://doc.rust-lang.org/stable/std/collections/struct.BinaryHeap.html,
/// except slow and buggy.
#[derive(Clone, Debug, Default)]
pub struct MyHeap<T> {
    data: Vec<T>,
}

impl<T: cmp::Ord> MyHeap<T> {
    pub fn new() -> Self {
        MyHeap { data: vec![] }
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.data.iter()
    }

    pub fn push(&mut self, value: T) {
        self.data.push(value);
        let mut index = self.data.len() - 1;
        while index > 0 {
            let parent = (index - 1) / 2;
            if self.data[parent] < self.data[index] {
                self.data.swap(index, parent);
                index = parent;
            } else {
                break;
            }
        }
    }

    // This implementation is wrong, because it doesn't preserve ordering
    pub fn pop_wrong(&mut self) -> Option<T> {
        if self.is_empty() {
            None
        } else {
            Some(self.data.swap_remove(0))
        }
    }

    // Fixed implementation of pop()
    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let ret = self.data.swap_remove(0);

        // Restore the heap property
        let mut index = 0;
        loop {
            let child1 = index * 2 + 1;
            let child2 = index * 2 + 2;
            if child1 >= self.data.len() {
                break;
            }

            let child = if child2 == self.data.len()
                || self.data[child1] > self.data[child2]
            {
                child1
            } else {
                child2
            };

            if self.data[index] < self.data[child] {
                self.data.swap(child, index);
                index = child;
            } else {
                break;
            }
        }

        Some(ret)
    }
}

use proptest::prelude::*;
use proptest::state_machine::{AbstractStateMachine, StateMachineTest};
use proptest::test_runner::Config;

#[derive(Clone, Debug)]
enum Transition {
    Pop,
    Push(i32),
}

trait SystemUnderTest<T> {
    fn pop(&mut self) -> Option<T>;
    fn push(&mut self, value: T);
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool;
}

impl SystemUnderTest<i32> for () {
    fn pop(&mut self) -> Option<i32> {
        None
    }

    fn push(&mut self, _value: i32) {}

    fn len(&self) -> usize {
        0
    }

    fn is_empty(&self) -> bool {
        false
    }
}

impl SystemUnderTest<i32> for MyHeap<i32> {
    fn pop(&mut self) -> Option<i32> {
        // switch to self.pop() to fix the bug
        self.pop_wrong()
    }

    fn push(&mut self, value: i32) {
        self.push(value)
    }

    fn len(&self) -> usize {
        self.len()
    }

    fn is_empty(&self) -> bool {
        self.is_empty()
    }
}

#[derive(Debug, Clone)]
struct MyModel<T> {
    sut: T,
    state: BinaryHeap<i32>,
    popped_sut: Option<i32>,
    len_sut: usize,
    empty_sut: bool,
    popped_state: Option<i32>,
    len_state: usize,
    empty_state: bool,
}

impl<T: SystemUnderTest<i32>> MyModel<T> {
    fn new(sut: T) -> Self {
        Self {
            sut,
            state: Default::default(),
            popped_sut: None,
            len_sut: 0,
            empty_sut: false,
            popped_state: None,
            len_state: 0,
            empty_state: false,
        }
    }

    fn transition(mut self, transition: &Transition) -> Self {
        match transition {
            Transition::Pop => {
                self.popped_sut = self.sut.pop();
                self.popped_state = self.state.pop();
            }
            Transition::Push(value) => {
                self.sut.push(*value);
                self.state.push(*value);
            }
        }
        self.len_state = self.state.len();
        self.empty_state = self.state.is_empty();
        self.len_sut = self.sut.len();
        self.empty_sut = self.sut.is_empty();

        self
    }

    fn invariants(&self) {
        assert_eq!(self.popped_state, self.popped_sut);
        assert_eq!(self.len_state, self.len_sut);
        assert_eq!(self.empty_state, self.empty_sut);
        assert_eq!(self.len_sut == 0, self.empty_sut);
    }
}

struct HeapStateMachine {}

impl AbstractStateMachine for HeapStateMachine {
    type State = MyModel<()>;
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(MyModel::new(())).boxed()
    }

    fn transitions(_state: &Self::State) -> BoxedStrategy<Self::Transition> {
        // The element can be given different weights.
        prop_oneof![
            1 => Just(Transition::Pop),
            2 => (any::<i32>()).prop_map(Transition::Push),
        ]
        .boxed()
    }

    fn apply_abstract(
        state: Self::State,
        transition: &Self::Transition,
    ) -> Self::State {
        state.transition(transition)
    }
}

struct MyHeapTest;
impl StateMachineTest for MyHeapTest {
    type ConcreteState = MyModel<MyHeap<i32>>;
    type Abstract = HeapStateMachine;

    fn init_test(
        _initial_state: <Self::Abstract as AbstractStateMachine>::State,
    ) -> Self::ConcreteState {
        MyModel::new(MyHeap::new())
    }

    fn apply_concrete(
        state: Self::ConcreteState,
        transition: &Transition,
    ) -> Self::ConcreteState {
        state.transition(transition)
    }

    fn invariants(state: &Self::ConcreteState, _transition: &Transition) {
        state.invariants()
    }
}

// Run the state machine test without the [`prop_state_machine`] macro
proptest! {
    #![proptest_config(Config {
        // Turn failure persistence off for demonstration
        failure_persistence: None,
        .. Config::default()
    })]
    // #[test]
    fn run_without_macro(
        (initial_state, transitions) in HeapStateMachine::sequential_strategy(1..20)
    ) {
        MyHeapTest::test_sequential(initial_state, transitions)
    }
}

// Run the state machine test using the [`prop_state_machine`] macro
prop_state_machine! {
    #![proptest_config(Config {
        // Turn failure persistence off for demonstration
        failure_persistence: None,
        .. Config::default()
    })]
    #[test]
    fn run_with_macro(sequential 1..10 => MyHeapTest);
}

fn main() {
    run_without_macro();
}
