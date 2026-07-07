use std::sync::atomic::{AtomicU32, Ordering};

pub type SymNodeId = u32;

/// A global counter for generating unique variable IDs.
pub(crate) static VAR_COUNTER: AtomicU32 = AtomicU32::new(1);

/// A node in the Symbolic Expression (AST) tree.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum SymExpr {
    /// A concrete value (constant)
    Const { val: u64, size: u32 },
    /// A symbolic variable (e.g. tainted input byte)
    Var { id: SymNodeId, name: String, size: u32 },
    
    // Arithmetic
    Add(Box<SymExpr>, Box<SymExpr>),
    Sub(Box<SymExpr>, Box<SymExpr>),
    Mul(Box<SymExpr>, Box<SymExpr>),
    Udiv(Box<SymExpr>, Box<SymExpr>),
    
    // Bitwise
    And(Box<SymExpr>, Box<SymExpr>),
    Or(Box<SymExpr>, Box<SymExpr>),
    Xor(Box<SymExpr>, Box<SymExpr>),
    Shl(Box<SymExpr>, Box<SymExpr>),
    Lshr(Box<SymExpr>, Box<SymExpr>),
    
    // Boolean / Comparison (returns 1-bit boolean expression)
    Eq(Box<SymExpr>, Box<SymExpr>),
    Neq(Box<SymExpr>, Box<SymExpr>),
    Ult(Box<SymExpr>, Box<SymExpr>),
    Ule(Box<SymExpr>, Box<SymExpr>),
    
    // Control Flow
    Ite { cond: Box<SymExpr>, t: Box<SymExpr>, f: Box<SymExpr> },
    
    // Bit extraction / concat
    Extract { expr: Box<SymExpr>, lsb: u32, size: u32 },
    Concat(Box<SymExpr>, Box<SymExpr>),
}

impl SymExpr {
    pub fn new_var(name: impl Into<String>, size: u32) -> Self {
        let id = VAR_COUNTER.fetch_add(1, Ordering::SeqCst);
        Self::Var { id, name: name.into(), size }
    }

    pub fn new_const(val: u64, size: u32) -> Self {
        Self::Const { val, size }
    }

    pub fn new_add(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => {
                let mask = if *size == 64 { u64::MAX } else { (1 << size) - 1 };
                Self::Const { val: (v1.wrapping_add(*v2)) & mask, size: *size }
            },
            (Self::Const { val: 0, .. }, _) => b,
            (_, Self::Const { val: 0, .. }) => a,
            _ => Self::Add(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_and(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => Self::Const { val: v1 & v2, size: *size },
            (Self::Const { val: 0, size }, _) => Self::Const { val: 0, size: *size },
            (_, Self::Const { val: 0, size }) => Self::Const { val: 0, size: *size },
            (a, b) if a == b => a.clone(),
            _ => Self::And(Box::new(a), Box::new(b)),
        }
    }

    pub fn new_xor(a: SymExpr, b: SymExpr) -> Self {
        match (&a, &b) {
            (Self::Const { val: v1, size }, Self::Const { val: v2, .. }) => Self::Const { val: v1 ^ v2, size: *size },
            (Self::Const { val: 0, .. }, _) => b,
            (_, Self::Const { val: 0, .. }) => a,
            (a, b) if a == b => Self::Const { val: 0, size: a.get_size() },
            _ => Self::Xor(Box::new(a), Box::new(b)),
        }
    }

    pub fn get_size(&self) -> u32 {
        match self {
            Self::Const { size, .. } => *size,
            Self::Var { size, .. } => *size,
            Self::Add(a, _) | Self::Sub(a, _) | Self::Mul(a, _) | Self::Udiv(a, _) => a.get_size(),
            Self::And(a, _) | Self::Or(a, _) | Self::Xor(a, _) | Self::Shl(a, _) | Self::Lshr(a, _) => a.get_size(),
            Self::Eq(_, _) | Self::Neq(_, _) | Self::Ult(_, _) | Self::Ule(_, _) => 1,
            Self::Ite { t, .. } => t.get_size(),
            Self::Extract { size, .. } => *size,
            Self::Concat(a, b) => a.get_size() + b.get_size(),
        }
    }
}
