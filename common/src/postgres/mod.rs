pub mod buffile;
pub mod catalog;
pub mod composite;
pub mod condition_variable;
pub mod cost;
pub mod customscan;
pub mod datetime;
pub mod jsonb_support;
pub mod locks;
pub mod node;
pub mod pdb_owned_value;
pub mod planner_warnings;
pub mod ps_status;
pub mod range;
pub mod sequentialscan {
    pub mod keyset;
    pub use keyset::KeySet;
}
pub mod tuplesort;
pub mod types;
pub mod types_arrow;
pub mod utils;
pub mod validate;

pub mod options {
    pub use crate::gucs::{MAX_MUTABLE_SEGMENT_ROWS, MAX_TARGET_SEGMENT_COUNT};
}

unsafe extern "C" {
    pub fn IsLogicalWorker() -> bool;
}
