use std::fmt;

#[derive(Clone, Copy, Debug)]
pub enum CmpOp {
    Lt,
    Le,
    Eq,
    Ne,
    Ge,
    Gt,
}

impl CmpOp {
    /// Compare two `u64` (or `usize as u64`).
    #[inline]
    pub fn eval_u64(self, lhs: u64, rhs: u64) -> bool {
        match self {
            CmpOp::Lt => lhs < rhs,
            CmpOp::Le => lhs <= rhs,
            CmpOp::Eq => lhs == rhs,
            CmpOp::Ne => lhs != rhs,
            CmpOp::Ge => lhs >= rhs,
            CmpOp::Gt => lhs > rhs,
        }
    }

    /// Compare two `f64`.
    #[inline]
    pub fn eval_f64(self, lhs: f64, rhs: f64) -> bool {
        match self {
            CmpOp::Lt => lhs < rhs,
            CmpOp::Le => lhs <= rhs,
            CmpOp::Eq => (lhs - rhs).abs() < f64::EPSILON,
            CmpOp::Ne => (lhs - rhs).abs() >= f64::EPSILON,
            CmpOp::Ge => lhs >= rhs,
            CmpOp::Gt => lhs > rhs,
        }
    }
}

impl fmt::Display for CmpOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sym = match self {
            CmpOp::Lt => "<",
            CmpOp::Le => "<=",
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Ge => ">=",
            CmpOp::Gt => ">",
        };
        write!(f, "{}", sym)
    }
}
