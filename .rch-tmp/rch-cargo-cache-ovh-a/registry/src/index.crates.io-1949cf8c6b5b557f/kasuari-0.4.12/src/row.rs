use hashbrown::hash_map::Entry;
use hashbrown::HashMap;

#[derive(Debug, Clone)]
pub struct Row {
    pub cells: HashMap<Symbol, f64>,
    pub constant: f64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol(usize, SymbolKind);

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolKind {
    Invalid,
    External,
    Slack,
    Error,
    Dummy,
}

impl Symbol {
    pub fn new(id: usize, kind: SymbolKind) -> Symbol {
        Symbol(id, kind)
    }

    pub fn invalid() -> Symbol {
        Symbol(0, SymbolKind::Invalid)
    }
    pub fn kind(&self) -> SymbolKind {
        self.1
    }
}

pub fn near_zero(value: f64) -> bool {
    const EPS: f64 = 1E-8;
    if value < 0.0 {
        -value < EPS
    } else {
        value < EPS
    }
}

impl Row {
    pub fn new(constant: f64) -> Row {
        Row {
            cells: HashMap::new(),
            constant,
        }
    }

    pub fn add(&mut self, v: f64) -> f64 {
        self.constant += v;
        self.constant
    }

    pub fn insert_symbol(&mut self, s: Symbol, coefficient: f64) {
        match self.cells.entry(s) {
            Entry::Vacant(entry) => {
                if !near_zero(coefficient) {
                    entry.insert(coefficient);
                }
            }
            Entry::Occupied(mut entry) => {
                *entry.get_mut() += coefficient;
                if near_zero(*entry.get_mut()) {
                    entry.remove();
                }
            }
        }
    }

    pub fn insert_row(&mut self, other: &Row, coefficient: f64) -> bool {
        let constant_diff = other.constant * coefficient;
        self.constant += constant_diff;
        for (s, v) in &other.cells {
            self.insert_symbol(*s, v * coefficient);
        }
        constant_diff != 0.0
    }

    pub fn remove(&mut self, s: Symbol) {
        self.cells.remove(&s);
    }

    pub fn reverse_sign(&mut self) {
        self.constant = -self.constant;
        for v in self.cells.values_mut() {
            *v = -*v;
        }
    }

    pub fn solve_for_symbol(&mut self, s: Symbol) {
        let coeff = -1.0
            / match self.cells.entry(s) {
                Entry::Occupied(entry) => entry.remove(),
                Entry::Vacant(_) => unreachable!(),
            };
        self.constant *= coeff;
        for v in self.cells.values_mut() {
            *v *= coeff;
        }
    }

    pub fn solve_for_symbols(&mut self, lhs: Symbol, rhs: Symbol) {
        self.insert_symbol(lhs, -1.0);
        self.solve_for_symbol(rhs);
    }

    pub fn coefficient_for(&self, s: Symbol) -> f64 {
        self.cells.get(&s).cloned().unwrap_or(0.0)
    }

    pub fn substitute(&mut self, s: Symbol, row: &Row) -> bool {
        if let Some(coeff) = self.cells.remove(&s) {
            self.insert_row(row, coeff)
        } else {
            false
        }
    }
}
