# vendor/gridcraft

The formula engine of xljs (`book.calculate()`, `cell.calc()`): a part of
[GridCraft](https://github.com/storytold/gridcraft), a spreadsheet application in Rust,
copied here and changed where Japanese Excel differs. `docs/design/calc-design.md` says how
xljs uses it and why it is copied rather than depended on.

- Source: https://github.com/storytold/gridcraft, commit
  `8bde636f272cb5ac1d9b945cecf17791febe605d` (2026-11-14, version 0.19.1).
- License: MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`). The regional format
  data in `core/src/region.rs` follows Unicode CLDR (Unicode License v3, in `NOTICE`).

## What is here

| Directory | GridCraft's | What it is |
|---|---|---|
| `core/` | `crates/core` | Cell addresses, values, errors, date serials |
| `formula/` | `crates/formula` | The formula language: lexer, parser, printer, reference adjustment |
| `functions/` | `crates/functions` | The worksheet functions (about 490) |
| `numfmt/` | `crates/numfmt` | Number formats, for `TEXT()` |
| `model/` | `crates/model` | The document model the engine calculates on |
| `calc/` | `crates/calc` | Evaluator, dependency graph, recalculation, spills |
| `xlsx-formulas/` | three files of `crates/xlsx` | How formulas are spelled in files (`_xlfn.`) and how legacy formulas are read (implicit intersection) |

Only `src/` of each crate is copied (no integration tests, benches or fixtures). The rest of
GridCraft (its xlsx reader and writer, charts, PDF, the engine of commands, the UI) is not here.

## Changes

Each change is marked with a comment that starts with `xljs:`.

- **Manifests** (`*/Cargo.toml`): written for this tree (GridCraft's inherit from its
  workspace). No `[lints]`.
- **`model/src/math.rs`** (new) stands in for the `gridcraft-math` crate (LaTeX equations laid
  out with RaTeX), which is not copied: no equation is laid out. `model/src/features.rs` uses it
  under the old name; the tests of the equation cache, which need the real layout, are removed.
- **`xlsx-formulas/`**: `src/lib.rs` is xljs's (the `FunctionInfo` type of `crates/xlsx/src/lib.rs`
  and a public `apply_legacy`); `array_formula.rs` and `fmla.rs` are unchanged; `tables.rs` is the
  list of functions that take the `_xlfn.` prefix, cut out of `crates/xlsx/src/tables.rs`.
- **`calc/src/recalc.rs`**: `Calc::seed`, so that `RAND()` does not draw the same numbers in
  every run.
- **`numfmt/src/parse.rs`, `render.rs`**: the Japanese date parts: `aaa` / `aaaa` (weekday),
  `g` / `gg` / `ggg` (era), `e` / `ee` (year in the era; GridCraft read `e` as the four-digit
  year) and `[$-ja-JP-x-gannen]` (元年). One assertion of `tests.rs` follows, with new ones.
- **`functions/src/text/japanese.rs`** (new): the byte functions (`LENB`, `LEFTB`, `RIGHTB`,
  `MIDB`, `REPLACEB`, `FINDB`, `SEARCHB`) count a full-width character as two bytes when the host
  asks (`gridcraft_functions::set_dbcs`); `ASC`, `JIS` and `DBCS` convert the width; `YEN` and
  `DATESTRING` are new. `functions/src/text.rs` registers them, `lib.rs` exports `set_dbcs`.

`NOW()` and `TODAY()` need no change: xljs gives the engine the local time (`Calc::fixed_now`).

## Building and testing

The crates are members of xljs's workspace. `cargo test --lib` tests xljs only; the crates' own
tests run with

    cargo test -p gridcraft-core -p gridcraft-formula -p gridcraft-functions -p gridcraft-numfmt \
               -p gridcraft-model -p gridcraft-calc -p gridcraft-xlsx-formulas

The engine recurses deeply and measures its stack use, so xljs's `Cargo.toml` optimizes these
crates in debug builds too (as GridCraft's own profile does); without that some of their tests
overflow the stack.

## Updating

1. Check out the new GridCraft commit and copy `crates/{core,formula,functions,numfmt,model,calc}/src`
   over the directories here, and `crates/xlsx/src/{array_formula,fmla}.rs` over
   `xlsx-formulas/src/`.
2. Put the changes above back (search the old tree for `xljs:`), and cut the list of prefixed
   functions out of `crates/xlsx/src/tables.rs` again.
3. Compare the dependencies in GridCraft's manifests with the ones here.
4. Run the tests above and `cargo test --lib xlsx_calc`, and note the new commit at the top.
