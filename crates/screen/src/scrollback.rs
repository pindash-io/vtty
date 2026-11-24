// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Bounded scrollback history: scrolled-off lines, oldest first, each with
//! its soft-wrap continuation flag so resize can reflow history too.

use alloc::collections::VecDeque;
use alloc::vec::Vec;

use crate::cell::Cell;

/// A bounded ring of scrolled-off lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Scrollback {
    /// Oldest-first lines: `(cells, continues_previous_line)`.
    lines: VecDeque<(Vec<Cell>, bool)>,
    /// Maximum capacity; `0` disables.
    max: usize,
}

impl Scrollback {
    /// An empty scrollback with capacity `max`.
    pub(crate) fn new(max: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max,
        }
    }

    /// The number of retained lines.
    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.lines.len()
    }

    /// The configured capacity (`0` disables).
    #[inline]
    pub(crate) const fn max(&self) -> usize {
        self.max
    }

    /// Drop all retained lines.
    pub(crate) fn clear(&mut self) {
        self.lines.clear();
    }

    /// Change the capacity, truncating the oldest lines if shrinking. `0`
    /// clears.
    pub(crate) fn set_max(&mut self, n: usize) {
        self.max = n;
        if n == 0 {
            self.lines.clear();
        } else {
            while self.lines.len() > n {
                self.lines.pop_front();
            }
        }
    }

    /// Retain a line, dropping the oldest when at capacity. Recycles the
    /// oldest line's allocation instead of freeing and re-allocating.
    pub(crate) fn push(&mut self, cells: &[Cell], cont: bool) {
        if self.max == 0 {
            return;
        }
        if self.lines.len() < self.max {
            self.lines.push_back((cells.to_vec(), cont));
            return;
        }
        // `len >= max >= 1`, so the queue is non-empty; the `let-else` keeps
        // the panic-free guarantee the crate promises.
        let Some(mut oldest) = self.lines.pop_front() else {
            return;
        };
        oldest.0.copy_from_slice(cells);
        oldest.1 = cont;
        self.lines.push_back(oldest);
    }

    /// The cells of line `i` (`0` = oldest), or `None` out of range.
    #[inline]
    pub(crate) fn line(&self, i: usize) -> Option<&[Cell]> {
        self.lines.get(i).map(|(row, _)| row.as_slice())
    }

    /// Iterate over `(cells, continuation)` in oldest-first order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &(Vec<Cell>, bool)> {
        self.lines.iter()
    }

    /// Replace the retained lines with `lines`, keeping the capacity.
    pub(crate) fn replace(&mut self, lines: Vec<(Vec<Cell>, bool)>) {
        self.lines = lines.into();
    }
}
