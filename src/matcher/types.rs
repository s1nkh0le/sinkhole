use strum_macros::Display;

#[derive(Clone, Copy, Debug, Display)]
pub enum CmpOp { Lt, Le, Eq, Ne, Ge, Gt }

impl CmpOp {
    /// Compare two `u64` (or `usize as u64`).
    #[inline]
    pub fn eval_u64(self, lhs: u64, rhs: u64) -> bool {
        match self {
            CmpOp::Lt => lhs <  rhs,
            CmpOp::Le => lhs <= rhs,
            CmpOp::Eq => lhs == rhs,
            CmpOp::Ne => lhs != rhs,
            CmpOp::Ge => lhs >= rhs,
            CmpOp::Gt => lhs >  rhs,
        }
    }

    /// Compare two `f64`.
    #[inline]
    pub fn eval_f64(self, lhs: f64, rhs: f64) -> bool {
        match self {
            CmpOp::Lt => lhs <  rhs,
            CmpOp::Le => lhs <= rhs,
            CmpOp::Eq => (lhs - rhs).abs() < f64::EPSILON,
            CmpOp::Ne => (lhs - rhs).abs() >= f64::EPSILON,
            CmpOp::Ge => lhs >= rhs,
            CmpOp::Gt => lhs >  rhs,
        }
    }
}
