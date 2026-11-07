pub mod expr_eval;
pub mod joinscan;
pub mod limit_offset;
pub mod mpp;
pub mod parameterized_value;

use pgrx::{IntoDatum, direct_function_call, pg_sys};
use std::ffi::CString;
use std::sync::OnceLock;

pub unsafe fn operator_oid(signature: &str) -> pg_sys::Oid {
    direct_function_call::<pg_sys::Oid>(
        pg_sys::regoperatorin,
        &[CString::new(signature).into_datum()],
    )
    .expect("should be able to lookup operator signature")
}

pub fn score_funcoids() -> [pg_sys::Oid; 2] {
    static OID_CACHE: OnceLock<[pg_sys::Oid; 2]> = OnceLock::new();
    *OID_CACHE.get_or_init(|| {
        [
            unsafe {
                direct_function_call::<pg_sys::Oid>(
                    pg_sys::regprocedurein,
                    &[c"pdb.score(anyelement)".into_datum()],
                )
                .expect("the `pdb.score(anyelement)` function should exist")
            },
            unsafe {
                direct_function_call::<pg_sys::Oid>(
                    pg_sys::regprocedurein,
                    &[c"paradedb.score(anyelement)".into_datum()],
                )
                .expect("the `paradedb.score(anyelement)` function should exist")
            },
        ]
    })
}

pub mod orderby {
    pub use crate::postgres::utils::text_lower_funcoid;
    use crate::schema::SearchFieldType;

    pub fn sortable_at_position(field_type: &SearchFieldType, position: usize) -> bool {
        position == 0 || !matches!(*field_type, SearchFieldType::Range(_))
    }
}
