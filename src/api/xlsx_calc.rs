//! The formula engine of a workbook: book.calculate(), cell.calc() and what
//! the print layout works out (conditional formatting, formulas without a
//! saved result, print areas given as formulas).
//!
//! The engine is GridCraft's (vendor/gridcraft). The workbook itself lives in
//! umya-spreadsheet, so its cells, names and tables are copied into the
//! engine's model the first time something is calculated; after that only
//! the cells scripts write are taken over, and what depends on them is
//! calculated again. Results go back as the saved values of the formula
//! cells (store). docs/design/calc-design.md has the reasoning.
//!
//! Formulas are those of dynamic-array Excel: one a script writes spills
//! when it gives several values, and is saved the way Excel saves it
//! (set_formula); the cells an array formula fills belong to it (Areas).

use std::cell::{Cell as StdCell, RefMut};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

use gridcraft_calc::eval::{Ev, Evaluator, Host};
use gridcraft_core::{CellError, CellRef, DateSystem, RangeRef, Value};
use gridcraft_formula::{Expr, SheetSel};
use gridcraft_model as gm;
use gridcraft_xlsx_formulas::{FunctionInfo, Names, SaveAs, apply_legacy, fmla, legacy_reading, save_as};
use umya_spreadsheet::{Cell, CellErrorType, CellFormula, CellFormulaValues, CellRawValue, Worksheet};

use super::xlsx::{Book, fmt_num, parse_ref};
use crate::{Error, Result, i18n, t};

/// A cell of the engine's model: (sheet index, position).
type Key = (usize, CellRef);

/// What the engine asks about worksheet functions when it reads formulas
/// saved without dynamic-array metadata.
const FUNCTIONS: FunctionInfo = FunctionInfo {
    is_builtin: |name| gridcraft_functions::lookup(name).is_some(),
    is_scalar_arg: |name, i| gridcraft_functions::lookup(name).is_none_or(|s| s.is_scalar_arg(i)),
};

/// Functions the engine has only a stand-in for: it would give an error (or,
/// for PHONETIC, the text itself: the readings are not kept) where the
/// workbook holds the value Excel worked out.
const STAND_INS: &[&str] = &["CUBEKPIMEMBER", "CUBEMEMBER", "CUBEMEMBERPROPERTY", "CUBERANKEDMEMBER", "CUBESET", "CUBESETCOUNT", "CUBEVALUE", "FILTERXML", "PHONETIC", "WEBSERVICE"];

/// Cells of array formula areas a workbook may have before the areas are no
/// longer followed cell by cell (their formulas then keep their saved values).
const MAX_AREA_CELLS: u64 = 2_000_000;

// ---- what scripts changed ----

/// The cells whose content changed since the engine last calculated (since
/// the workbook was opened, when it has not yet).
#[derive(Default)]
pub(crate) struct Touched {
    /// More changed than is followed cell by cell, or something other than
    /// the content of cells: everything is calculated again.
    all: bool,
    sheets: Vec<String>,
    /// (index in sheets, column, row)
    cells: Vec<(u32, u32, u32)>,
}

const MAX_TOUCHED: usize = 200_000;

impl Touched {
    pub fn cell(&mut self, sheet: &str, col: u32, row: u32) {
        if self.all {
            return;
        }
        if self.cells.len() >= MAX_TOUCHED {
            return self.everything();
        }
        let i = match self.sheets.iter().rposition(|s| s == sheet) {
            Some(i) => i,
            None => {
                self.sheets.push(sheet.to_string());
                self.sheets.len() - 1
            }
        };
        self.cells.push((i as u32, col, row));
    }

    pub fn everything(&mut self) {
        *self = Touched { all: true, ..Default::default() };
    }

    fn any(&self) -> bool {
        self.all || !self.cells.is_empty()
    }
}

// ---- the cells array formulas fill ----

/// The areas the array formulas of a sheet fill beyond their own cell, by
/// first cell: (row, column) -> (last row, last column), 1-based. The cells
/// in them hold what Excel (or store) laid out there and belong to the
/// formula: the file says so with the range of the formula alone, and Excel
/// lays its result over them again.
#[derive(Default)]
pub(crate) struct Areas {
    map: BTreeMap<(u32, u32), (u32, u32)>,
    /// The most rows an area has: how far above a cell its first cell can be.
    tallest: u32,
}

impl Areas {
    fn scan(ws: &Worksheet) -> Areas {
        let mut areas = Areas::default();
        for c in ws.cells() {
            let Some(f) = c.formula_obj().filter(|f| *f.formula_type() == CellFormulaValues::Array) else { continue };
            let at = at_of(c);
            if let Some(r) = RangeRef::parse(f.reference()).filter(|r| r.start == at && !r.is_single() && r.end.is_valid()) {
                areas.tallest = areas.tallest.max(r.end.row - r.start.row + 1);
                areas.map.insert((r.start.row + 1, r.start.col + 1), (r.end.row + 1, r.end.col + 1));
            }
        }
        areas
    }

    /// The area a cell is in: its first and its last cell.
    fn around(&self, col: u32, row: u32) -> Option<((u32, u32), (u32, u32))> {
        if self.map.is_empty() {
            return None;
        }
        let from = row.saturating_sub(self.tallest.saturating_sub(1));
        self.map.range((from, 0)..=(row, col)).find(|((_, c1), (r2, c2))| *c1 <= col && col <= *c2 && row <= *r2).map(|(first, last)| (*first, *last))
    }
}

/// Before a script writes into a cell. When the cell is in the area of an
/// array formula, the area is given up, as in Excel when something is typed
/// over a spilled value: the values the formula filled in go, and the
/// formula keeps its own cell (it gives #SPILL! when it is calculated and
/// the written cell is in its way). Without this the file would still say
/// that the area is the formula's, and Excel would lay the result over what
/// the script wrote. Writing over the formula itself removes its values too.
pub(crate) fn release_area(b: &mut Book, sheet: &str, col: u32, row: u32) {
    let Book { wb, areas, touched, .. } = b;
    let Ok(ws) = wb.sheet_by_name_mut(sheet) else { return };
    if !areas.contains_key(sheet) {
        areas.insert(sheet.to_string(), Areas::scan(ws));
    }
    let Some(areas) = areas.get_mut(sheet) else { return };
    let Some((first, last)) = areas.around(col, row) else { return };
    areas.map.remove(&first);
    let touched = touched.get_mut();
    for r in first.0..=last.0 {
        for c in first.1..=last.1 {
            if (r, c) == first || (r, c) == (row, col) {
                continue;
            }
            if ws.cell((c, r)).is_some_and(|x| !x.is_formula() && !x.raw_value().is_empty()) {
                ws.cell_mut((c, r)).cell_value_mut().set_blank();
                touched.cell(sheet, c, r);
            }
        }
    }
    if first != (row, col) {
        let c = ws.cell_mut((first.1, first.0));
        if let Some(mut f) = c.formula_obj().cloned() {
            f.set_reference(CellRef::new(first.0 - 1, first.1 - 1).a1());
            c.cell_value_mut().set_formula_obj(f);
        }
        touched.cell(sheet, first.1, first.0);
    }
}

// ---- the clock ----

thread_local! {
    /// The time NOW() and TODAY() give instead of the clock's (a test run).
    static FIXED_NOW: StdCell<Option<f64>> = const { StdCell::new(None) };
}

/// Fixes the time the formulas of this thread's scripts see ("2026-04-10T09:00:00",
/// local wall clock time), or frees it again.
pub(crate) fn set_fixed_now(now: Option<&str>) {
    let serial = now.and_then(|s| {
        let s = s.trim();
        let t = ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M"].iter().find_map(|f| chrono::NaiveDateTime::parse_from_str(s, f).ok());
        let t = t.or_else(|| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)))?;
        Some(serial_of(t))
    });
    FIXED_NOW.with(|n| n.set(serial));
}

fn serial_of(t: chrono::NaiveDateTime) -> f64 {
    use chrono::{Datelike, Timelike};
    super::xlsx::to_serial(t.year(), t.month(), t.day(), t.hour(), t.minute(), t.second(), t.nanosecond() / 1_000_000, false)
}

/// Now as a serial number of the 1900 date system, in local time (the engine
/// converts it for a 1904 workbook).
fn now_serial() -> f64 {
    FIXED_NOW.with(|n| n.get()).unwrap_or_else(|| serial_of(chrono::Local::now().naive_local()))
}

/// Where RAND() starts: the clock, or always the same in a test run.
fn seed() -> u64 {
    if FIXED_NOW.with(|n| n.get()).is_some() {
        return 0;
    }
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0)
}

// ---- values ----

fn error_of(e: &CellErrorType) -> CellError {
    CellError::parse(&e.to_string()).unwrap_or(CellError::Value)
}

/// The error as umya-spreadsheet can save it (it knows the errors of Excel 2007).
fn error_to(e: CellError) -> CellErrorType {
    match e {
        CellError::Null => CellErrorType::Null,
        CellError::Div0 => CellErrorType::Div0,
        CellError::Ref => CellErrorType::Ref,
        CellError::Name => CellErrorType::Name,
        CellError::Num => CellErrorType::Num,
        CellError::NA | CellError::GettingData => CellErrorType::NA,
        _ => CellErrorType::Value,
    }
}

/// The value of a cell for the engine. formula: the cell has one, so an
/// empty text is a result ("") and not a blank cell.
fn value_of(c: &Cell, formula: bool) -> Value {
    match c.raw_value() {
        CellRawValue::Empty => Value::Empty,
        CellRawValue::Numeric(n) => Value::number(*n),
        CellRawValue::Bool(v) => Value::Bool(*v),
        // A picture placed in the cell is #VALUE! to formulas, as in Excel
        // (cell_image finds the formulas that show it).
        CellRawValue::Error(e) => Value::Error(error_of(e)),
        other => {
            let s = other.to_string();
            if s.is_empty() && !formula { Value::Empty } else { Value::text(s) }
        }
    }
}

/// Whether the saved value of a cell is v.
fn same(raw: &CellRawValue, v: &Value) -> bool {
    match (raw, v) {
        (CellRawValue::Empty, Value::Empty) => true,
        (CellRawValue::Numeric(a), Value::Number(b)) => a == b,
        (CellRawValue::Bool(a), Value::Bool(b)) => a == b,
        (CellRawValue::Error(a), Value::Error(b)) => *a == error_to(*b),
        (CellRawValue::String(a) | CellRawValue::Lazy(a), Value::Text(b)) => **a == **b,
        (CellRawValue::RichText(a), Value::Text(b)) => *a.text() == **b,
        _ => false,
    }
}

/// Saves v as the result of the formula of c (the formula stays).
fn set_result(c: &mut Cell, v: &Value) {
    match v {
        Value::Empty => c.set_formula_result_blank(),
        Value::Number(n) => c.set_formula_result_number(*n),
        Value::Bool(x) => c.set_formula_result_bool(*x),
        Value::Error(e) => c.set_formula_result_error(error_to(*e)),
        Value::Text(s) => c.set_formula_result_string(s.to_string()),
        Value::Array(_) => return set_result(c, &v.scalar()),
    };
}

/// Saves v as the value of a cell an array formula fills.
fn set_plain(c: &mut Cell, v: &Value) {
    match v {
        Value::Empty => c.cell_value_mut().set_blank(),
        Value::Number(n) => c.cell_value_mut().set_value_number(*n),
        Value::Bool(x) => c.cell_value_mut().set_value_bool(*x),
        Value::Error(e) => c.cell_value_mut().set_formula_result_error(error_to(*e)),
        Value::Text(s) => c.cell_value_mut().set_value_string(s.to_string()),
        Value::Array(_) => return set_plain(c, &v.scalar()),
    };
}

fn at_of(c: &Cell) -> CellRef {
    let co = c.coordinate();
    CellRef::new(co.row_num().saturating_sub(1), co.col_num().saturating_sub(1))
}

fn coord(at: CellRef) -> (u32, u32) {
    (at.col + 1, at.row + 1)
}

// ---- formulas the engine leaves alone ----

/// Why a formula keeps the value it was saved with.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Skip {
    /// The engine cannot read the text (what it says about it).
    Syntax(String),
    /// A function it does not have: an add-in's, a VBA function, ...
    Function(String),
    /// A name the workbook does not define.
    Name(String),
    /// A reference to another workbook.
    External,
    /// A what-if data table.
    DataTable,
    /// An array formula over more cells than are followed.
    Large,
    /// The formula reads its own result, directly or through other formulas.
    Circular,
}

impl Skip {
    /// The reason as scripts get it.
    pub fn reason(&self) -> &'static str {
        match self {
            Skip::Syntax(_) => "syntax",
            Skip::Function(_) => "function",
            Skip::Name(_) => "name",
            Skip::External => "external",
            Skip::DataTable => "dataTable",
            Skip::Large => "large",
            Skip::Circular => "circular",
        }
    }

    /// The function or name it is about.
    pub fn name(&self) -> Option<&str> {
        match self {
            Skip::Function(n) | Skip::Name(n) => Some(n),
            _ => None,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Skip::Syntax(m) => t!("xlsx.calcSkip.syntax", m),
            Skip::Function(n) => t!("xlsx.calcSkip.function", n),
            Skip::Name(n) => t!("xlsx.calcSkip.name", n),
            Skip::External => t!("xlsx.calcSkip.external"),
            Skip::DataTable => t!("xlsx.calcSkip.dataTable"),
            Skip::Large => t!("xlsx.calcSkip.large"),
            Skip::Circular => t!("xlsx.calcSkip.circular"),
        }
    }

    /// What cell.calc() gives for such a formula that has no saved value.
    fn error(&self) -> &'static str {
        match self {
            Skip::External => "#REF!",
            Skip::DataTable | Skip::Large | Skip::Circular => "#VALUE!",
            _ => "#NAME?",
        }
    }
}

/// Why the engine would not work out e as Excel does (None: it does).
fn unsupported(wb: &gm::Workbook, sheet: usize, e: &Expr) -> Option<Skip> {
    // The names LET and LAMBDA declare, anywhere in the formula.
    let mut locals: Vec<&str> = Vec::new();
    e.walk(&mut |x| {
        if let Expr::Call(name, args) = x
            && (name == "LET" || name == "LAMBDA")
        {
            let last = args.len().saturating_sub(1);
            for (i, a) in args.iter().enumerate() {
                if let Expr::Name(n) = a
                    && i < last
                    && (name == "LAMBDA" || i % 2 == 0)
                {
                    locals.push(n);
                }
            }
        }
    });
    let local = |n: &str| locals.iter().any(|l| gridcraft_formula::same_name(l, n));
    let external = |s: &str| s.contains('[');
    let check = |x: &Expr| -> Option<Skip> {
        match x {
            Expr::Call(name, _) => {
                let known = gridcraft_calc::is_known_function(name) && !STAND_INS.contains(&name.to_ascii_uppercase().as_str());
                if known || local(name) || wb.name(name, sheet).is_some() { None } else { Some(Skip::Function(name.clone())) }
            }
            Expr::Ref(r) => match &r.sheet {
                SheetSel::Named(s) if external(s) => Some(Skip::External),
                SheetSel::Span(a, z) if external(a) || external(z) => Some(Skip::External),
                _ => None,
            },
            Expr::Name(n) => {
                // "Sheet1!Name" is a name of that sheet.
                let (scope, bare) = match n.rsplit_once('!') {
                    Some((s, _)) if external(s) => return Some(Skip::External),
                    Some((s, bare)) => (wb.sheet_index(&gridcraft_core::unquote_sheet(s)).unwrap_or(sheet), bare),
                    None => (sheet, n.as_str()),
                };
                if local(bare) || wb.name(bare, scope).is_some() || wb.table(bare).is_some() { None } else { Some(Skip::Name(n.clone())) }
            }
            Expr::Struct(st) if !st.table.is_empty() && wb.table(&st.table).is_none() => Some(Skip::Name(st.table.clone())),
            _ => None,
        }
    };
    let mut found: Option<Skip> = None;
    e.walk(&mut |x| {
        if found.is_none() {
            found = check(x);
        }
    });
    found
}

// ---- the engine's copy of the workbook ----

/// How a formula cell was saved.
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    /// Without dynamic-array metadata: Excel intersects ranges implicitly.
    Legacy,
    /// With it (a formula that spills), over this area when it was saved.
    Dynamic(RangeRef),
    /// Entered with Ctrl+Shift+Enter over this area.
    Array(RangeRef),
}

/// A cell of the workbook as the engine takes it.
struct Made {
    cell: gm::Cell,
    kind: Kind,
    skip: Option<Skip>,
}

/// Converts a cell. A formula the engine cannot read comes back without one
/// (the saved value stays) and with the reason.
fn make(c: &Cell) -> Made {
    let plain = |skip: Option<Skip>| Made { cell: gm::Cell::value(value_of(c, skip.is_some())), kind: Kind::Legacy, skip };
    let Some(f) = c.formula_obj() else { return plain(None) };
    let text = f.text().trim();
    if *f.formula_type() == CellFormulaValues::DataTable {
        return plain(Some(Skip::DataTable));
    }
    if text.is_empty() {
        // A cell of a shared formula whose first cell is gone.
        return plain(None);
    }
    let body = text.strip_prefix('=').unwrap_or(text);
    if let Err(e) = gridcraft_formula::parse(body) {
        return plain(Some(Skip::Syntax(e.msg)));
    }
    let mut formula = gm::Formula::new(&fmla::from_file(body));
    let at = at_of(c);
    let dynamic = f.dynamic_array();
    let kind = if *f.formula_type() == CellFormulaValues::Array {
        // The formula is in the first cell of the area it fills.
        let area = RangeRef::parse(f.reference()).filter(|r| r.start == at).unwrap_or(RangeRef::cell(at));
        if dynamic {
            Kind::Dynamic(area)
        } else {
            formula.array = Some(area);
            Kind::Array(area)
        }
    } else if dynamic {
        Kind::Dynamic(RangeRef::cell(at))
    } else {
        Kind::Legacy
    };
    formula.dynamic = dynamic;
    Made { cell: gm::Cell { value: value_of(c, true), formula: Some(Arc::new(formula)), ..Default::default() }, kind, skip: None }
}

/// What a sheet has besides its cells that formulas depend on: tables,
/// merged cells, hidden rows.
fn layout(ws: &Worksheet, id: &mut u32) -> gm::Sheet {
    let mut sh = gm::Sheet::new(ws.name());
    let range = |s: &str| RangeRef::parse(&s.replace('$', ""));
    for m in ws.merge_cells() {
        if let Some(r) = range(&m.range()) {
            sh.merges.push(r);
        }
    }
    let mut hidden: Vec<u32> = Vec::new();
    for r in ws.row_dimensions() {
        if r.hidden() && r.row_num() >= 1 {
            hidden.push(r.row_num() - 1);
            sh.rows.insert(r.row_num() - 1, gm::LineInfo { hidden: true, ..Default::default() });
        }
    }
    // The criteria of a filter are not kept: rows hidden inside its range
    // count as filtered out (SUBTOTAL leaves those out, unlike rows hidden by hand).
    let filtered = |r: &RangeRef| hidden.iter().any(|h| *h > r.start.row && *h <= r.end.row);
    let criterion = || vec![(0u32, gm::FilterCriterion::Values { values: Vec::new(), blanks: false })];
    if let Some(r) = ws.auto_filter().and_then(|f| range(&f.range().range())) {
        sh.autofilter = Some(gm::AutoFilter { range: r, criteria: if filtered(&r) { criterion() } else { Vec::new() } });
    }
    for t in ws.tables() {
        if !t.is_ok() {
            continue;
        }
        let (a, z) = t.area();
        let r = RangeRef::new(CellRef::new(a.row_num() - 1, a.col_num() - 1), CellRef::new(z.row_num() - 1, z.col_num() - 1));
        *id += 1;
        sh.tables.push(gm::Table {
            id: *id,
            name: (if t.display_name().is_empty() { t.name() } else { t.display_name() }).to_string(),
            range: r,
            // headerRowCount is not kept; tables nearly always have the row.
            header_row: true,
            totals_row: t.totals_row_shown() || t.totals_row_count() > 0,
            columns: t
                .columns()
                .iter()
                .map(|c| gm::TableColumn { name: c.name().to_string(), totals: Default::default(), totals_label: None, formula: None, unique_name: None, xml: None })
                .collect(),
            style: String::new(),
            banded_rows: false,
            banded_cols: false,
            first_col: false,
            last_col: false,
            filter_button: true,
            filter: if filtered(&r) { criterion() } else { Vec::new() },
            table_type: None,
            connection_id: None,
        });
    }
    sh
}

/// The engine's copy of a workbook with its calculation state.
pub(crate) struct Engine {
    wb: gm::Workbook,
    calc: gridcraft_calc::Calc,
    /// The formula cells that keep the value they were saved with, by
    /// (sheet, row, column), and why.
    skipped: BTreeMap<(usize, u32, u32), Skip>,
    /// The areas the array formulas fill in the workbook's own cells, by
    /// sheet and first cell: what store() empties when an array gets smaller.
    areas: Vec<BTreeMap<CellRef, RangeRef>>,
    /// Formulas without a saved value: calculated the first time, whatever changed.
    fresh: Vec<Key>,
}

/// The workbook for the engine without its cells: the sheets with what
/// formulas depend on besides cells, the names, the calculation options.
fn outline(b: &Book) -> gm::Workbook {
    let mut wb = gm::Workbook::new();
    wb.date_system = if b.date1904 { DateSystem::D1904 } else { DateSystem::D1900 };
    // File > Options > Formulas > "Enable iterative calculation": circular
    // references are worked out again and again instead of being an error.
    let options = b.wb.calculation_properties();
    wb.calc.iterative = options.iterate();
    wb.calc.set_max_iterations(options.iterate_count());
    wb.calc.set_max_change(options.iterate_delta());
    let sheets = b.wb.sheet_collection();
    let mut id = 0;
    wb.sheets = sheets.iter().map(|ws| Arc::new(layout(ws, &mut id))).collect();
    // Names: those of the workbook, and those umya-spreadsheet keeps with
    // the sheet they refer to (of the sheet only when they say so).
    let mut seen: HashSet<(String, Option<usize>)> = HashSet::new();
    let global = b.wb.defined_names().iter().map(|d| (None, d));
    let local = sheets.iter().enumerate().flat_map(|(i, ws)| ws.defined_names().iter().map(move |d| (d.has_local_sheet_id().then_some(i), d)));
    for (scope, d) in global.chain(local) {
        let name = d.name();
        if name.is_empty() || name.to_ascii_lowercase().starts_with("_xlnm.") || !seen.insert((name.to_lowercase(), scope)) {
            continue;
        }
        wb.names.push(gm::DefinedName { name: name.to_string(), scope, formula: fmla::from_file(&d.address()), comment: String::new(), hidden: d.hidden() });
    }
    wb
}

impl Engine {
    fn build(b: &Book) -> Engine {
        let sheets = b.wb.sheet_collection();
        let mut e = Engine { wb: outline(b), calc: gridcraft_calc::Calc::new(), skipped: BTreeMap::new(), areas: vec![BTreeMap::new(); sheets.len()], fresh: Vec::new() };
        e.calc.seed(seed());
        let mut formulas: Vec<(Key, Kind)> = Vec::new();
        // The values of the cells array formulas fill (other than their first
        // cell): the engine lays them out itself.
        let mut filled: Vec<(usize, CellRef, CellRef, Value)> = Vec::new();
        let mut budget = MAX_AREA_CELLS;
        for (si, ws) in sheets.iter().enumerate() {
            let cells = ws.cells();
            // cell -> first cell of the array formula that fills it
            let mut cover: HashMap<CellRef, CellRef> = HashMap::new();
            let mut large: HashSet<CellRef> = HashSet::new();
            for c in &cells {
                let Some(f) = c.formula_obj().filter(|f| *f.formula_type() == CellFormulaValues::Array) else { continue };
                let at = at_of(c);
                let Some(r) = RangeRef::parse(f.reference()).filter(|r| r.start == at && !r.is_single()) else { continue };
                if r.count() > budget {
                    large.insert(at);
                    continue;
                }
                budget -= r.count();
                cover.extend(r.iter().filter(|p| *p != at).map(|p| (p, at)));
            }
            let Some(sh) = e.wb.sheet_mut(si) else { continue };
            for c in &cells {
                let at = at_of(c);
                if !c.is_formula()
                    && let Some(first) = cover.get(&at)
                {
                    let v = value_of(c, false);
                    if !v.is_empty() {
                        filled.push((si, at, *first, v));
                    }
                    continue;
                }
                let mut m = make(c);
                if large.contains(&at) && m.cell.formula.is_some() {
                    m.cell.formula = None;
                    m.skip = Some(Skip::Large);
                }
                if let Some(skip) = m.skip {
                    e.skipped.insert((si, at.row, at.col), skip);
                }
                if m.cell.formula.is_some() {
                    formulas.push(((si, at), m.kind));
                }
                if m.cell.has_content() {
                    sh.cells.set(at, m.cell);
                }
            }
        }
        // With every name and table in place: the formulas the engine would
        // not work out as Excel does keep their saved values.
        let mut legacy: Vec<Vec<CellRef>> = vec![Vec::new(); sheets.len()];
        for ((si, at), kind) in formulas {
            if e.settle(si, at) {
                continue;
            }
            match kind {
                Kind::Legacy => legacy[si].push(at),
                Kind::Array(r) | Kind::Dynamic(r) if !r.is_single() => {
                    e.areas[si].insert(at, r);
                    if let Some(sh) = e.wb.sheet_mut(si) {
                        sh.spill_ranges.insert(at, r);
                    }
                }
                _ => {}
            }
            if e.wb.sheet(si).and_then(|s| s.cell(at)).is_some_and(|c| c.value.is_empty()) {
                e.fresh.push((si, at));
            }
        }
        for (si, at, first, v) in filled {
            let Some(sh) = e.wb.sheet_mut(si) else { continue };
            if sh.spill_ranges.contains_key(&first) {
                sh.spill.insert(at, v);
            } else {
                // Its formula is not calculated: the values stay as they are.
                sh.cells.set(at, gm::Cell::value(v));
            }
        }
        apply_legacy(&mut e.wb, legacy.into_iter().enumerate().filter(|(_, v)| !v.is_empty()).collect(), FUNCTIONS);
        e.calc.rebuild(&e.wb);
        e
    }

    /// Checks the formula of a cell against the workbook's names and tables.
    /// One the engine would not work out as Excel does loses its formula here
    /// (the saved value stays): true then.
    fn settle(&mut self, si: usize, at: CellRef) -> bool {
        let Some(expr) = self.wb.sheet(si).and_then(|s| s.cell(at)).and_then(|c| c.formula.as_ref()).and_then(|f| f.expr_arc()) else { return true };
        let Some(skip) = unsupported(&self.wb, si, &expr) else { return false };
        self.skipped.insert((si, at.row, at.col), skip);
        if let Some(sh) = self.wb.sheet_mut(si) {
            match sh.cells.get_mut(at) {
                Some(c) if !c.value.is_empty() => c.formula = None,
                _ => {
                    sh.remove_cell(at);
                }
            }
        }
        true
    }

    /// Takes over the cells scripts wrote. None: the copy no longer matches
    /// the workbook and has to be made again.
    fn take_in(&mut self, b: &Book, touched: &Touched) -> Option<Vec<Key>> {
        let sheets: Vec<(usize, &Worksheet)> = touched.sheets.iter().map(|n| Some((self.wb.sheet_index(n)?, b.wb.sheet_by_name(n).ok()?))).collect::<Option<_>>()?;
        let mut cells = touched.cells.clone();
        cells.sort_unstable();
        cells.dedup();
        let mut keys = Vec::with_capacity(cells.len());
        let mut legacy: BTreeMap<usize, Vec<CellRef>> = BTreeMap::new();
        for (i, col, row) in cells {
            let (si, ws) = *sheets.get(i as usize)?;
            let at = CellRef::new(row.checked_sub(1)?, col.checked_sub(1)?);
            self.skipped.remove(&(si, at.row, at.col));
            let made = ws.cell((col, row)).map(make);
            let sh = self.wb.sheet_mut(si)?;
            match made {
                Some(m) => {
                    if let Some(skip) = m.skip {
                        self.skipped.insert((si, at.row, at.col), skip);
                    }
                    let formula = m.cell.formula.is_some();
                    if m.cell.has_content() {
                        sh.cells.set(at, m.cell);
                    } else {
                        sh.remove_cell(at);
                    }
                    if formula && !self.settle(si, at) && m.kind == Kind::Legacy {
                        legacy.entry(si).or_default().push(at);
                    }
                }
                None => {
                    sh.remove_cell(at);
                }
            }
            keys.push((si, at));
        }
        apply_legacy(&mut self.wb, legacy.into_iter().collect(), FUNCTIONS);
        Some(keys)
    }

    fn sheet(&self, name: &str) -> Option<(usize, &gm::Sheet)> {
        let si = self.wb.sheet_index(name)?;
        Some((si, self.wb.sheet(si)?))
    }

    /// The value the engine has for a cell.
    fn value(&self, sheet: &str, col: u32, row: u32) -> Value {
        match (self.sheet(sheet), col.checked_sub(1), row.checked_sub(1)) {
            (Some((_, sh)), Some(c), Some(r)) => sh.value(CellRef::new(r, c)),
            _ => Value::Empty,
        }
    }

    /// Works out a formula as if it were in the cell at, without storing it.
    fn eval(&self, si: usize, at: CellRef, e: &Expr) -> Ev {
        let mut host = Reader { wb: &self.wb, now: now_serial(), rng: seed() | 1 };
        Evaluator::new(&mut host, si, at).eval(e)
    }

    fn eval_value(&self, si: usize, at: CellRef, e: &Expr) -> Value {
        let mut host = Reader { wb: &self.wb, now: now_serial(), rng: seed() | 1 };
        Evaluator::new(&mut host, si, at).value(e)
    }
}

/// Reads the engine's copy for a formula worked out on the side (its cells
/// are up to date: nothing is calculated on the way).
struct Reader<'a> {
    wb: &'a gm::Workbook,
    now: f64,
    rng: u64,
}

impl Host for Reader<'_> {
    fn workbook(&self) -> &gm::Workbook {
        self.wb
    }
    fn cell_value(&mut self, sheet: usize, c: CellRef) -> Value {
        self.wb.sheet(sheet).map(|s| s.value(c)).unwrap_or_default()
    }
    fn now_serial(&self) -> f64 {
        self.now
    }
    fn random(&mut self) -> f64 {
        // xorshift64*, as the engine's own.
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Brings the engine up to date with the workbook and gives it: it is made
/// when there is none, the cells written since are taken over, and what
/// depends on them is calculated again (all: every formula).
fn engine(b: &Book, all: bool) -> RefMut<'_, Engine> {
    let mut slot = b.calc.borrow_mut();
    let mut touched = b.touched.borrow_mut();
    let mut all = all || touched.all;
    if touched.all {
        *slot = None;
    }
    let mut keys = Vec::new();
    if let Some(e) = slot.as_mut() {
        match e.take_in(b, &touched) {
            Some(k) => keys = k,
            // The copy does not match the workbook any more.
            None => (*slot, all) = (None, true),
        }
    }
    let made = slot.is_none();
    if made {
        let mut e = Engine::build(b);
        keys = std::mem::take(&mut e.fresh);
        match e.take_in(b, &touched) {
            Some(k) => keys.extend(k),
            None => all = true,
        }
        *slot = Some(Box::new(e));
    }
    let any = made || touched.any();
    *touched = Touched::default();
    drop(touched);
    let mut e = RefMut::map(slot, |s| &mut **s.as_mut().expect("the engine was just made"));
    e.calc.fixed_now = Some(now_serial());
    // LENB and the like count bytes as the Excel of the language does.
    gridcraft_functions::set_dbcs(i18n::current().excel.dbcs);
    if all {
        let e = &mut *e;
        e.calc.recalc_all(&mut e.wb);
    } else if any {
        // Also calculates the formulas that change on their own (NOW, RAND, OFFSET).
        let e = &mut *e;
        e.calc.cells_changed(&mut e.wb, &keys);
    }
    e
}

/// What the engine gives a formula on a circular reference, and those that
/// read one (Excel shows 0 and warns; the engine does not iterate).
fn circular(v: &Value) -> bool {
    *v == Value::Error(CellError::Circ)
}

/// Whether a sheet has a formula.
fn has_formulas(b: &Book) -> bool {
    b.wb.sheet_collection().iter().any(|ws| ws.cells().iter().any(|c| c.is_formula()))
}

/// Saves what the engine worked out as the values of the workbook's cells:
/// the results of the formulas and the cells array formulas fill. Returns how
/// many cells changed.
fn store(b: &mut Book) -> usize {
    let Book { wb, calc, areas: written, .. } = b;
    let Some(e) = calc.get_mut().as_deref_mut() else { return 0 };
    let Engine { wb: model, areas, .. } = e;
    let mut changed = 0;
    for (si, sh) in model.sheets.iter().enumerate() {
        let Ok(ws) = wb.sheet_mut(si) else { continue };
        for (at, cell) in sh.cells.iter() {
            if cell.formula.is_none() {
                continue;
            }
            // A circular reference has no result: the saved value stays.
            if circular(&cell.value) {
                continue;
            }
            let c = ws.cell_mut(coord(at));
            if c.is_formula() && !same(c.raw_value(), &cell.value) {
                set_result(c, &cell.value);
                // The picture Excel saved as the result (`vm`) is not the
                // result any more.
                c.set_value_meta_index(0);
                changed += 1;
            }
        }
        for (at, v) in &sh.spill {
            if circular(v) {
                continue;
            }
            let c = ws.cell_mut(coord(*at));
            if !c.is_formula() && !same(c.raw_value(), v) {
                set_plain(c, v);
                changed += 1;
            }
        }
        // What an array no longer fills is emptied, and the formula's own
        // note of its area follows.
        let Some(areas) = areas.get_mut(si) else { continue };
        for (first, old) in areas.iter() {
            let new = sh.spill_ranges.get(first).copied().unwrap_or(RangeRef::cell(*first));
            if new == *old {
                continue;
            }
            for at in old.iter().filter(|p| !new.contains(*p) && !sh.cells.has(*p) && !sh.spill.contains_key(p)) {
                if let Some(c) = ws.cell(coord(at))
                    && !c.is_formula()
                    && !c.raw_value().is_empty()
                {
                    ws.cell_mut(coord(at)).cell_value_mut().set_blank();
                    changed += 1;
                }
            }
        }
        let firsts: HashSet<CellRef> = areas.keys().chain(sh.spill_ranges.keys()).copied().collect();
        for first in firsts {
            let new = sh.spill_ranges.get(&first).copied().unwrap_or(RangeRef::cell(first));
            if areas.get(&first) == Some(&new) {
                continue;
            }
            let c = ws.cell_mut(coord(first));
            if let Some(mut f) = c.formula_obj().filter(|f| *f.formula_type() == CellFormulaValues::Array).cloned() {
                f.set_reference(new.a1());
                c.cell_value_mut().set_formula_obj(f);
            }
        }
        if *areas != sh.spill_ranges {
            // The ranges of the formulas changed: read again before the next write.
            written.remove(ws.name());
            *areas = sh.spill_ranges.clone();
        }
    }
    changed
}

// ---- book.calculate() ----

/// A formula that kept its saved value.
pub(crate) struct Skipped {
    pub sheet: String,
    pub address: String,
    pub formula: String,
    pub skip: Skip,
}

pub(crate) struct Summary {
    /// Formulas the engine worked out.
    pub formulas: usize,
    /// Cells whose saved value changed.
    pub changed: usize,
    pub skipped: Vec<Skipped>,
}

/// Calculates every formula of the workbook and saves the results in its cells.
pub(crate) fn calculate(b: &mut Book) -> Summary {
    let (formulas, skipped) = {
        let e = engine(b, true);
        let mut skipped: BTreeMap<(usize, u32, u32), Skip> = e.skipped.clone();
        let mut formulas = 0;
        for (si, sh) in e.wb.sheets.iter().enumerate() {
            for (at, c) in sh.cells.iter().filter(|(_, c)| c.formula.is_some()) {
                if circular(&c.value) {
                    skipped.insert((si, at.row, at.col), Skip::Circular);
                } else {
                    formulas += 1;
                }
            }
        }
        (formulas, skipped)
    };
    let changed = store(b);
    let skipped = skipped
        .into_iter()
        .filter_map(|((si, row, col), skip)| {
            let ws = b.wb.sheet(si).ok()?;
            let formula = ws.cell((col + 1, row + 1)).map(|c| c.formula().to_string()).unwrap_or_default();
            Some(Skipped { sheet: ws.name().to_string(), address: CellRef::new(row, col).a1(), formula: format!("={}", formula.trim_start_matches('=')), skip })
        })
        .collect();
    Summary { formulas, changed, skipped }
}

/// Before the workbook is printed: the formulas that depend on what scripts
/// wrote (and those without a saved value, and NOW and the like) are
/// calculated and their results saved, so that the pages show them.
pub(crate) fn refresh(b: &mut Book) {
    if b.calc.borrow().is_none() && !has_formulas(b) {
        return;
    }
    drop(engine(b, false));
    store(b);
}

// ---- cell.calc() and the values the print layout asks for ----

/// A value worked out for conditional formatting (xlsx_cond.rs).
#[derive(Clone, Debug, PartialEq)]
pub enum CalcVal {
    Num(f64),
    Str(String),
    Bool(bool),
    Empty,
    Err,
}

impl From<Value> for CalcVal {
    fn from(v: Value) -> CalcVal {
        match v.scalar() {
            Value::Number(n) => CalcVal::Num(n),
            Value::Text(s) => CalcVal::Str(s.to_string()),
            Value::Bool(b) => CalcVal::Bool(b),
            Value::Empty => CalcVal::Empty,
            _ => CalcVal::Err,
        }
    }
}

/// A formula read once, to be worked out for many cells (eval_at).
pub struct Prepared(Expr);

pub fn prepare(formula: &str) -> Option<Prepared> {
    let body = formula.trim();
    let body = fmla::from_file(body.strip_prefix('=').unwrap_or(body));
    gridcraft_formula::parse(&body).ok().map(Prepared)
}

/// The value of a prepared formula for the cell at (column, row): it was
/// written for the cell dc columns to the left and dr rows above, and its
/// relative references move with it.
pub fn eval_at(b: &Book, sheet: &str, f: &Prepared, at: (u32, u32), dc: i64, dr: i64) -> CalcVal {
    let e = engine(b, false);
    let Some((si, _)) = e.sheet(sheet) else { return CalcVal::Err };
    let pos = CellRef::new(at.1.saturating_sub(1), at.0.saturating_sub(1));
    if dc == 0 && dr == 0 {
        return e.eval_value(si, pos, &f.0).into();
    }
    e.eval_value(si, pos, &gridcraft_formula::adjust::shift_relative(f.0.clone(), dr, dc)).into()
}

/// The value of a cell (a formula without a saved result is worked out).
pub fn value_at(b: &Book, sheet: &str, col: u32, row: u32) -> CalcVal {
    let Some(c) = b.sheet(sheet).ok().and_then(|ws| ws.cell((col, row))) else { return CalcVal::Empty };
    if c.is_formula() && c.raw_value().is_empty() {
        return engine(b, false).value(sheet, col, row).into();
    }
    value_of(c, c.is_formula()).into()
}

/// The value of the formula in the cell, as text: worked out now, from the
/// cells as they are.
pub fn calc_cell(b: &Book, sheet: &str, r: &str) -> Result<String> {
    let (col, row) = parse_ref(r)?;
    let ws = b.sheet(sheet)?;
    let Some(c) = ws.cell((col, row)) else { return Ok(String::new()) };
    if !c.is_formula() {
        return Ok(text_of(&value_of(c, false)));
    }
    let e = engine(b, false);
    let Some((si, sh)) = e.sheet(sheet) else { return Ok(String::new()) };
    let at = CellRef::new(row - 1, col - 1);
    if let Some(skip) = e.skipped.get(&(si, at.row, at.col)) {
        return match skip {
            Skip::Syntax(m) => Err(Error::msg(format!("formula: {m}"))),
            _ if c.raw_value().is_empty() => Ok(skip.error().to_string()),
            _ => Ok(text_of(&sh.value(at))),
        };
    }
    let v = match sh.cell(at).and_then(|c| c.formula.as_ref()) {
        // A formula on a circular reference has no result.
        _ if circular(&sh.value(at)) => sh.value(at),
        // An array formula's own cell shows the first of its values.
        Some(f) if f.array.is_none() => match f.expr_arc() {
            Some(expr) => match e.eval_value(si, at, &expr) {
                // Several values with something in their way: the formula cannot spill.
                Value::Array(_) if sh.value(at) == Value::Error(CellError::Spill) => sh.value(at),
                v => v,
            },
            None => sh.value(at),
        },
        _ => sh.value(at),
    };
    if circular(&v.scalar()) {
        // Like the other formulas that are not calculated: the saved value.
        return Ok(if c.raw_value().is_empty() { Skip::Circular.error().to_string() } else { text_of(&value_of(c, true)) });
    }
    Ok(text_of(&v))
}

/// A value as cell.calc() gives it: like Excel shows it in the General format.
fn text_of(v: &Value) -> String {
    match v.scalar() {
        Value::Number(n) => fmt_calc(n),
        Value::Error(e) => e.as_str().to_string(),
        v => v.to_text().unwrap_or_default(),
    }
}

/// The picture a cell shows: one placed in it, or what its formula gives
/// (`=B2`, `=Sheet1!B2`, `=IF(...,B2)`, `=XLOOKUP(...)`), also for a formula
/// written by a script that has no saved result. None when it shows none.
pub fn cell_image(b: &Book, sheet: &str, col: u32, row: u32) -> Option<usize> {
    if b.cell_images.images.is_empty() {
        return None;
    }
    let c = b.sheet(sheet).ok()?.cell((col, row))?;
    match c.raw_value() {
        CellRawValue::Error(_) => {
            if let Some(i) = b.cell_images.of(c) {
                return Some(i);
            }
        }
        CellRawValue::Empty => {}
        _ => return None,
    }
    if !c.is_formula() {
        return None;
    }
    let e = engine(b, false);
    let (si, _) = e.sheet(sheet)?;
    Pictures { e: &e, b, number: false, seen: HashMap::new() }.shown((si, CellRef::new(row - 1, col - 1)), 0)
}

/// Finds the picture a formula hands on. To the engine a picture is #VALUE!,
/// like any formula that works with one (`=B2&""`, `=B2+1`); a formula that
/// only passes it on (`=B2`, IF, the lookups) is told apart by working it
/// out twice with the pictures standing in as values: once as a text and
/// once as a number that nothing else gives. Only a formula that leaves both
/// as they are shows the picture.
struct Pictures<'a> {
    e: &'a Engine,
    b: &'a Book,
    /// The pictures stand in as numbers (else as texts).
    number: bool,
    /// The formulas worked out so far.
    seen: HashMap<(Key, bool), Value>,
}

/// Formulas that hand a picture on from one to the next.
const MAX_PICTURE_DEPTH: usize = 32;

impl Pictures<'_> {
    const TEXT: &'static str = "\u{E000}xljs-picture:";
    const NUMBER: f64 = 8.25e200;

    fn stand_in(&self, i: usize) -> Value {
        if self.number { Value::Number(Self::NUMBER + i as f64 * 1e186) } else { Value::text(format!("{}{i}", Self::TEXT)) }
    }

    fn picture(&self, v: &Value) -> Option<usize> {
        let n = self.b.cell_images.images.len();
        match v {
            Value::Text(t) if !self.number => t.strip_prefix(Self::TEXT)?.parse().ok().filter(|i| *i < n),
            Value::Number(x) if self.number => (0..n).find(|i| *x == Self::NUMBER + *i as f64 * 1e186),
            _ => None,
        }
    }

    /// The picture the formula cell shows, when both ways agree.
    fn shown(&mut self, k: Key, depth: usize) -> Option<usize> {
        self.number = false;
        let text = self.formula(k, depth);
        let as_text = self.picture(&text)?;
        self.number = true;
        let number = self.formula(k, depth);
        let as_number = self.picture(&number);
        self.number = false;
        as_number.filter(|i| *i == as_text)
    }

    /// The value of a formula cell with the pictures standing in.
    fn formula(&mut self, k: Key, depth: usize) -> Value {
        if let Some(v) = self.seen.get(&(k, self.number)) {
            return v.clone();
        }
        let error = Value::Error(CellError::Value);
        let Some(expr) = self.e.wb.sheet(k.0).and_then(|s| s.cell(k.1)).and_then(|c| c.formula.as_ref()).and_then(|f| f.expr_arc()) else { return error };
        // Until it is worked out: a formula that reads itself shows none.
        self.seen.insert((k, self.number), error.clone());
        let mut host = PictureHost { p: self, depth };
        let v = Evaluator::new(&mut host, k.0, k.1).value(&expr).scalar();
        self.seen.insert((k, self.number), v.clone());
        v
    }

    /// A cell as a formula reads it.
    fn cell(&mut self, sheet: usize, c: CellRef, depth: usize) -> Value {
        let Some(sh) = self.e.wb.sheet(sheet) else { return Value::Empty };
        let v = sh.value(c);
        // Only a cell that is #VALUE! can be a picture.
        if v != Value::Error(CellError::Value) {
            return v;
        }
        if let Some(i) = self.b.sheet(&sh.name).ok().and_then(|ws| ws.cell((c.col + 1, c.row + 1))).and_then(|x| self.b.cell_images.of(x)) {
            return self.stand_in(i);
        }
        if depth >= MAX_PICTURE_DEPTH || !sh.cell(c).is_some_and(|x| x.formula.is_some()) {
            return v;
        }
        let got = self.formula((sheet, c), depth + 1);
        if self.picture(&got).is_some() { got } else { v }
    }
}

struct PictureHost<'p, 'a> {
    p: &'p mut Pictures<'a>,
    depth: usize,
}

impl Host for PictureHost<'_, '_> {
    fn workbook(&self) -> &gm::Workbook {
        &self.p.e.wb
    }
    fn cell_value(&mut self, sheet: usize, c: CellRef) -> Value {
        self.p.cell(sheet, c, self.depth)
    }
    fn now_serial(&self) -> f64 {
        now_serial()
    }
    fn random(&mut self) -> f64 {
        0.5
    }
}

/// The cells a reference formula points at, such as a dynamic print area
/// `OFFSET(Sheet1!$A$1,0,0,MAX(...),7)` or a plain `Sheet1!$A$1:$G$40`
/// (1-based: c1, r1, c2, r2). None when it is something else or out of the sheet.
pub fn eval_area(b: &Book, sheet: &str, formula: &str) -> Option<(u32, u32, u32, u32)> {
    let f = prepare(formula)?;
    let e = engine(b, false);
    let (si, _) = e.sheet(sheet)?;
    let Ev::R(areas) = e.eval(si, CellRef::new(0, 0), &f.0) else { return None };
    let [a] = areas.as_slice() else { return None };
    let r = a.range;
    if r.is_full_rows() || r.is_full_cols() || !r.end.is_valid() {
        return None;
    }
    Some((r.start.col + 1, r.start.row + 1, r.end.col + 1, r.end.row + 1))
}

// ---- formulas as scripts write and read them ----

/// What the formulas scripts write and read are checked against: the
/// workbook's sheets, names and tables, without its cells.
pub(crate) struct Shape {
    wb: gm::Workbook,
    names: Names,
}

fn shape(b: &Book) -> RefMut<'_, Shape> {
    let mut slot = b.shape.borrow_mut();
    if slot.is_none() {
        *slot = Some(Box::new(Shape { wb: outline(b), names: Names::default() }));
    }
    RefMut::map(slot, |s| &mut **s.as_mut().expect("the shape was just made"))
}

/// Writes a formula into a cell as Excel takes one that is typed in: a
/// formula that gives several values (`FILTER(...)`, `A1:A3*2`) is a dynamic
/// array formula and spills, and an `@` asks for one value of several. The
/// file holds it as Excel saves it: an array formula with dynamic-array
/// metadata when evaluating it the old way (implicit intersection) would
/// give something else, a plain formula otherwise. An empty text removes the
/// formula and the value.
pub(crate) fn set_formula(b: &mut Book, sheet: &str, col: u32, row: u32, text: &str) -> Result<()> {
    let body = text.trim();
    let body = body.strip_prefix('=').unwrap_or(body);
    let saved = (!body.trim().is_empty()).then(|| file_form(b, sheet, col, row, body));
    let c = b.cell_mut(sheet, col, row)?;
    // The value the cell had is not the formula's.
    c.cell_value_mut().set_blank();
    let Some((text, dynamic)) = saved else { return Ok(()) };
    if dynamic {
        // Its range is its own cell until it is calculated (store), here or
        // by Excel when it opens the file.
        let mut f = CellFormula::default();
        f.set_text(text);
        f.set_formula_type(CellFormulaValues::Array);
        f.set_dynamic_array(true);
        c.cell_value_mut().set_formula_obj(f);
    } else {
        c.set_formula(text);
    }
    // Excel works the results out when it opens the file.
    b.wb.calculation_properties_mut().set_full_calc_on_load(true);
    Ok(())
}

/// A formula a script wrote as the file holds it, and whether it is a
/// dynamic array formula there.
fn file_form(b: &Book, sheet: &str, col: u32, row: u32, body: &str) -> (String, bool) {
    // A prefix the script wrote itself (`_xlfn.`) says nothing about the formula.
    let Ok(expr) = gridcraft_formula::parse(&formula_from_file(body)) else { return (formula_to_file(body), false) };
    let mut shape = shape(b);
    let Shape { wb, names } = &mut *shape;
    let Some(si) = wb.sheet_index(sheet) else { return (formula_to_file(body), false) };
    let at = CellRef::new(row.saturating_sub(1), col.saturating_sub(1));
    match save_as(wb, si, at, &gm::Formula::from_expr(expr), FUNCTIONS, names) {
        SaveAs::Unchanged => (formula_to_file(body), false),
        // Every `@` is one Excel implies in a formula without the metadata.
        SaveAs::Legacy(e) => (fmla::to_file(&gm::Formula::from_expr(e)), false),
        SaveAs::Dynamic => (formula_to_file(body), true),
    }
}

/// The formula of a cell as Excel shows it, and as a script would write it
/// to get the same (no `=`): without the prefixes of the file, and with `@`
/// where a formula saved without dynamic-array metadata takes one value of
/// several (`@A1:A5`, `@Names`). None when the cell has no formula.
pub(crate) fn formula_shown(b: &Book, sheet: &str, col: u32, row: u32) -> Option<String> {
    let c = b.sheet(sheet).ok()?.cell((col, row))?;
    let f = c.formula_obj()?;
    if f.text().trim().is_empty() {
        return None;
    }
    let shown = formula_from_file(f.text());
    // Array formulas work with every value as they are written.
    if f.dynamic_array() || *f.formula_type() != CellFormulaValues::Normal && *f.formula_type() != CellFormulaValues::Shared {
        return Some(shown);
    }
    let formula = gm::Formula::new(&shown);
    if formula.expr.is_none() {
        return Some(shown);
    }
    let mut shape = shape(b);
    let Shape { wb, names } = &mut *shape;
    let Some(si) = wb.sheet_index(sheet) else { return Some(shown) };
    let at = CellRef::new(row.saturating_sub(1), col.saturating_sub(1));
    let shown = legacy_reading(wb, si, at, &formula, FUNCTIONS, names).map_or(shown, |read| read.text);
    Some(this_row_shown(&shown, own_table(b, sheet, col, row).as_deref()))
}

/// The table whose data rows hold the cell.
fn own_table(b: &Book, sheet: &str, col: u32, row: u32) -> Option<String> {
    let ws = b.sheet(sheet).ok()?;
    ws.tables().iter().find_map(|t| {
        let (a, z) = t.area();
        let inside = (a.col_num()..=z.col_num()).contains(&col) && (a.row_num() + 1..=z.row_num()).contains(&row);
        inside.then(|| (if t.display_name().is_empty() { t.name() } else { t.display_name() }).to_string())
    })
}

/// The references to the row of the formula in a table as Excel shows them:
/// `果物[[#This Row],[数量]]` (as the file holds it) is `[@数量]` in the table
/// itself and `果物[@数量]` elsewhere; `[@[単 価]]` for a name with spaces or
/// signs, `[@[数量]:[単価]]` for several columns. Strings stay as they are.
fn this_row_shown(text: &str, own: Option<&str>) -> String {
    static THIS_ROW: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)([\p{L}_\\][\p{L}\p{N}_.]*)?\[\[#This Row\],\s*(\[[^\[\]]*\](?::\[[^\[\]]*\])?|[^\[\]]+)\]").unwrap()
    });
    if !text.to_ascii_lowercase().contains("#this row") {
        return text.to_string();
    }
    let simple = |name: &str| name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.');
    text.split('"')
        .enumerate()
        .map(|(i, part)| {
            if i % 2 == 1 {
                return part.to_string();
            }
            THIS_ROW
                .replace_all(part, |c: &regex::Captures| {
                    let table = c.get(1).map(|m| m.as_str());
                    let cols = c[2].trim();
                    let inner = if cols.starts_with('[') {
                        match cols.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
                            Some(one) if !one.contains("]:[") && simple(one) => one.to_string(),
                            _ => cols.to_string(),
                        }
                    } else if simple(cols) {
                        cols.to_string()
                    } else {
                        format!("[{cols}]")
                    };
                    let prefix = match table {
                        Some(t) if own.is_none_or(|o| !o.eq_ignore_ascii_case(t)) => t,
                        _ => "",
                    };
                    format!("{prefix}[@{inner}]")
                })
                .into_owned()
        })
        .collect::<Vec<_>>()
        .join("\"")
}

// ---- formulas as files spell them ----

/// A formula a script wrote, as the file has to hold it: Excel reads the
/// functions newer than Excel 2007 only with their prefix (`_xlfn.XLOOKUP`)
/// and shows #NAME? without it. Formulas that need nothing stay as written.
pub fn formula_to_file(text: &str) -> String {
    let body = text.strip_prefix('=').unwrap_or(text);
    let Ok(e) = gridcraft_formula::parse(body) else { return body.to_string() };
    let mut differs = false;
    e.walk(&mut |x| {
        differs |= match x {
            Expr::Call(name, _) => name == "LET" || name == "LAMBDA" || gridcraft_xlsx_formulas::tables::file_function_name(name).is_some(),
            Expr::Unary(gridcraft_formula::UnOp::Spill | gridcraft_formula::UnOp::At | gridcraft_formula::UnOp::Trim(_), _) => true,
            Expr::Struct(_) => true,
            _ => false,
        }
    });
    // A prefix the script wrote itself is kept by the rewrite too.
    if differs { fmla::text_to_file(body) } else { body.to_string() }
}

/// A formula of the file as Excel shows it (and scripts write it): without
/// the prefixes.
pub fn formula_from_file(text: &str) -> String {
    let body = text.strip_prefix('=').unwrap_or(text);
    if !body.to_ascii_lowercase().contains("_xl") {
        return body.to_string();
    }
    fmla::from_file(body)
}

/// A computed number as text (no binary noise: 15 significant digits).
fn fmt_calc(n: f64) -> String {
    let s = format!("{:.15e}", n);
    let v: f64 = s.parse().unwrap_or(n);
    let r: f64 = format!("{v:.15}").parse().unwrap_or(v);
    fmt_num(super::numfmt::general(r).parse().unwrap_or(r))
}

#[cfg(test)]
mod tests;
