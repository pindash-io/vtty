// Copyright © Fangdun Tsai <fundon@pindash.io>
// SPDX-License-Identifier: Apache-2.0 OR MIT

//! Soft-wrap reflow helpers (pure functions over row lists).

use alloc::vec;
use alloc::vec::Vec;

use vtty_width::cell_width;

use crate::cell::{CONTINUATION, Cell};
use crate::render::trailing_blanks;

/// Build a `(row, continuation)` list from a flat grid.
pub(crate) fn rows_from(
    cells: &[Cell],
    continuation: &[bool],
    cols: u32,
) -> Vec<(Vec<Cell>, bool)> {
    cells
        .chunks_exact(cols as usize)
        .zip(continuation)
        .map(|(row, &cont)| (row.to_vec(), cont))
        .collect()
}

/// Re-wrap a row list at `width`, dropping trailing blank rows (viewport
/// padding) so content stays anchored to the top when the grid shrinks.
pub(crate) fn reflow_rows(rows: &[(Vec<Cell>, bool)], width: usize) -> Vec<(Vec<Cell>, bool)> {
    let mut reflowed = rewrap_rows(rows, width);
    while reflowed
        .last()
        .is_some_and(|(row, _)| row.iter().all(Cell::is_blank))
    {
        reflowed.pop();
    }
    reflowed
}

/// Fit reflowed rows into a `width × rows` grid, keeping the last `rows`
/// rows and their continuation flags.
pub(crate) fn fit_grid(
    reflowed: &[(Vec<Cell>, bool)],
    width: usize,
    rows: usize,
) -> (Vec<Cell>, Vec<bool>) {
    let take = reflowed.len().min(rows);
    let skip = reflowed.len() - take;
    let mut cells = vec![Cell::default(); width * rows];
    let mut cont = vec![false; rows];
    for i in 0..take {
        let (row, c) = &reflowed[skip + i];
        let dst = i * width;
        cells[dst..dst + width].copy_from_slice(row);
        cont[i] = *c;
    }
    (cells, cont)
}

/// Extract logical lines from a row list, joining soft-wrapped rows and
/// dropping double-width continuation markers.
fn logical_lines_rows(rows: &[(Vec<Cell>, bool)]) -> Vec<Vec<Cell>> {
    let mut lines = Vec::new();
    let mut current: Vec<Cell> = Vec::new();
    for i in 0..rows.len() {
        let (cells, cont) = &rows[i];
        if i > 0 && !cont {
            lines.push(core::mem::take(&mut current));
        }
        // Trim trailing padding blanks — but only on the last row of a
        // logical line. A soft-wrapped row is full of content, so its
        // trailing spaces (e.g. the space before the wrap in "hello
        // world") must be kept.
        let soft_wrapped = rows.get(i + 1).is_some_and(|(_, next)| *next);
        let end = if soft_wrapped {
            cells.len()
        } else {
            cells.len() - trailing_blanks(cells)
        };
        current.extend(
            cells[..end]
                .iter()
                .copied()
                .filter(|c| !c.is_continuation()),
        );
    }
    lines.push(current);
    lines
}

/// Re-wrap a list of `(row, continuation)` pairs at `width`, re-deriving
/// the continuation flags from the new layout.
fn rewrap_rows(rows: &[(Vec<Cell>, bool)], width: usize) -> Vec<(Vec<Cell>, bool)> {
    let mut out = Vec::new();
    for line in logical_lines_rows(rows) {
        for (i, row) in rewrap(&line, width).into_iter().enumerate() {
            out.push((row, i > 0));
        }
    }
    out
}

/// Re-wrap a logical line (lead cells only, wide glyphs as one entry) into
/// `width`-cell rows, re-adding double-width continuation markers.
fn rewrap(line: &[Cell], width: usize) -> Vec<Vec<Cell>> {
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    let mut row: Vec<Cell> = Vec::with_capacity(width);
    for &cell in line {
        let mut w = cell_width(cell.ch).unwrap_or(1);
        if w == 0 {
            continue;
        }
        if w == 2 && width < 2 {
            w = 1;
        }
        if row.len() >= width {
            rows.push(core::mem::take(&mut row));
            row = Vec::with_capacity(width);
        }
        // A wide glyph needs two cells; if only one remains, wrap first.
        if w == 2 && row.len() == width - 1 {
            row.resize(width, Cell::default());
            rows.push(row);
            row = Vec::with_capacity(width);
        }
        row.push(cell);
        if w == 2 {
            row.push(Cell {
                ch: CONTINUATION,
                ..cell
            });
        }
    }
    if !row.is_empty() || rows.is_empty() {
        row.resize(width, Cell::default());
        rows.push(row);
    }
    rows
}
