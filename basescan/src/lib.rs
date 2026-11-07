// Copyright (c) 2023-2026 ParadeDB, Inc.
//
// This file is part of ParadeDB - Postgres for Search and Analytics

#![allow(
    unsafe_op_in_unsafe_fn,
    clippy::missing_safety_doc,
    clippy::not_unsafe_ptr_arg_deref,
    rustdoc::private_intra_doc_links,
    rustdoc::broken_intra_doc_links
)]

pub use ::scan::scan;
pub use planner::*;

pub mod postgres;

pub use postgres::customscan;
