//! Local fsqlite compat helpers.
//!
//! Bridges the parameter-binding gap between rusqlite's `params!` (binds by
//! reference) and fsqlite's compat `params!` (consumes by value via
//! `ParamValue::From<T>`). Exposes the [`ToParam`] trait and a local
//! `params!` macro that calls `ToParam::to_param()` on each argument so we
//! can keep the rusqlite-era `params![record.field, ...]` patterns
//! verbatim across storage call sites.

use fsqlite::compat::ParamValue;
use fsqlite_types::value::SqliteValue;

/// Convert a value reference into an fsqlite `ParamValue` without consuming
/// it. Mirrors the borrow-friendly behaviour of `rusqlite::ToSql`.
pub trait ToParam {
    fn to_param(&self) -> ParamValue;
}

impl ToParam for str {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(self)
    }
}
impl ToParam for String {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(self.as_str())
    }
}
impl ToParam for i64 {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(*self)
    }
}
impl ToParam for i32 {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(*self)
    }
}
impl ToParam for u32 {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(*self)
    }
}
impl ToParam for u64 {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(*self)
    }
}
impl ToParam for usize {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(*self)
    }
}
impl ToParam for f64 {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(*self)
    }
}
impl ToParam for f32 {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(f64::from(*self))
    }
}
impl ToParam for bool {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(*self)
    }
}
impl ToParam for [u8] {
    fn to_param(&self) -> ParamValue {
        ParamValue::from(self)
    }
}
impl ToParam for Vec<u8> {
    fn to_param(&self) -> ParamValue {
        // Bind through the &[u8] path so the BLOB lands in a single
        // `Arc::from(slice)` allocation instead of cloning the Vec into a
        // fresh owned buffer first and then reboxing it into the Arc.
        // This shaves one full-buffer clone per embedding INSERT.
        ParamValue::from(self.as_slice())
    }
}
impl<T: ToParam + ?Sized> ToParam for &T {
    fn to_param(&self) -> ParamValue {
        (**self).to_param()
    }
}
impl<T: ToParam> ToParam for Option<T> {
    fn to_param(&self) -> ParamValue {
        match self {
            Some(inner) => inner.to_param(),
            None => ParamValue(SqliteValue::Null),
        }
    }
}

/// rusqlite-style `params!` that binds by reference (does not consume).
///
/// Wraps [`ToParam::to_param()`] for each argument so call-sites can keep
/// using `params![record.field, other.field]` against borrowed structs.
#[macro_export]
macro_rules! ms_params {
    () => { &[] as &[$crate::storage::sqlite_compat::__re::ParamValue] };
    ($($val:expr),+ $(,)?) => {
        &[
            $(
                $crate::storage::sqlite_compat::ToParam::to_param(&$val)
            ),+
        ] as &[$crate::storage::sqlite_compat::__re::ParamValue]
    };
}

/// Internal re-exports used by the `ms_params!` macro so call-sites do not
/// need to import `ParamValue` explicitly.
#[doc(hidden)]
pub mod __re {
    pub use fsqlite::compat::ParamValue;
}
