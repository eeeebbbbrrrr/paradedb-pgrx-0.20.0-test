// Copyright (c) 2023-2026 ParadeDB, Inc.
//
// This file is part of ParadeDB - Postgres for Search and Analytics
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see <http://www.gnu.org/licenses/>.
#![recursion_limit = "512"]
// Edition 2024 turns this lint on by default. This crate is a thin Rust layer over the Postgres C
// API, so nearly every function here is `unsafe fn` whose body is unsafe end to end; wrapping each
// body in a blanket `unsafe {}` would add noise without adding granularity. Left as follow-up work
// to be tightened per-module, where the wrapped region can be narrowed to something meaningful.
#![allow(unsafe_op_in_unsafe_fn)]

// Direct dep, not just via `dst`: a cdylib drops the transitively-referenced shim, so reference it
// here to keep its `.init_array` constructor.
#[cfg(feature = "dst")]
use antithesis_instrumentation as _;

pub use common::{
    DEFAULT_PARAMETERIZED_LIMIT_ESTIMATE, DEFAULT_STARTUP_COST, EXPENSIVE_QUERY_COST_FACTOR,
    FULL_RELATION_SELECTIVITY, FUZZY_HIGH_SELECTIVITY, FUZZY_LOW_SELECTIVITY,
    MORE_LIKE_THIS_SELECTIVITY, PARAMETERIZED_SELECTIVITY, REGEX_SELECTIVITY,
    UNASSIGNED_SELECTIVITY, UNKNOWN_SELECTIVITY, available_parallelism, gucs, impl_safe_drop,
    nodecast, parallel_worker, vector,
};
pub use index_read::{query, schema};
pub use index_write::index;
pub use joinscan::scan;
pub use planner::aggregate;

mod api;
mod bootstrap;
mod postgres;

use self::postgres::customscan;
use pgrx::*;

#[repr(transparent)]
struct AnchorPtr(*const ());
unsafe impl Sync for AnchorPtr {}

#[used]
static _SUBCRATE_PGRX_ANCHORS: [AnchorPtr; 21] = [
    AnchorPtr(common::api::fieldname_in as *const ()),
    AnchorPtr(common::api::fieldname_out as *const ()),
    AnchorPtr(common::api::tokenizers::typmod::generic_typmod_in as *const ()),
    AnchorPtr(common::api::tokenizers::typmod::generic_typmod_out as *const ()),
    AnchorPtr(common::postgres::types::tantivyvalue_in as *const ()),
    AnchorPtr(common::postgres::types::tantivyvalue_out as *const ()),
    AnchorPtr(index_read::query::searchqueryinput_in as *const ()),
    AnchorPtr(index_read::query::searchqueryinput_out as *const ()),
    AnchorPtr(index_read::query::pdb_query::pdb::query_in as *const ()),
    AnchorPtr(index_read::query::pdb_query::pdb::query_out as *const ()),
    AnchorPtr(index_read::query::proximity::proximityclause_in as *const ()),
    AnchorPtr(index_read::query::proximity::proximityclause_out as *const ()),
    AnchorPtr(planner::postgres::customscan::projections::placeholder_support as *const ()),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::score::pdb::score_from_relation
            as *const (),
    ),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::score::paradedb_score_from_relation
            as *const (),
    ),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::snippet::pdb::snippet_from_relation
            as *const (),
    ),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::snippet::pdb::snippets_from_relation
            as *const (),
    ),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::snippet::pdb::snippet_positions_from_relation
            as *const (),
    ),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::snippet::paradedb_snippet_from_relation
            as *const (),
    ),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::snippet::paradedb_snippets_from_relation
            as *const (),
    ),
    AnchorPtr(
        planner::postgres::customscan::basescan::projections::snippet::paradedb_snippet_positions_from_relation
            as *const (),
    ),
];

pgrx::pg_module_magic!();

extension_sql!(
    r#"
        -- PUBLIC needs USAGE to reference the extension's objects (types, casts,
        -- operators, functions in `pdb`, and helpers in `paradedb`), but not
        -- CREATE: every object in these schemas is created here at install time
        -- by the extension owner, so granting CREATE to PUBLIC would only let
        -- any role squat objects inside the extension's own schemas.
        GRANT USAGE ON SCHEMA paradedb TO PUBLIC;
        GRANT USAGE ON SCHEMA pdb TO PUBLIC;
    "#,
    name = "paradedb_grant_usage",
    finalize
);

/// Initializes option parsing
fn check_for_interrupts() {
    pgrx::check_for_interrupts!();
}

#[allow(clippy::missing_safety_doc)]
#[allow(non_snake_case)]
#[pg_guard]
pub unsafe extern "C-unwind" fn _PG_init() {
    // Optional: initialize env_logger for dependency debug output (tantivy, lindera, etc.)
    // Enable with: cargo build --features debug-logging
    // Without this feature, log! macros (to stderr) in dependencies become no-ops.
    // We can't implement our own logger that sends messages to Postgres `ereport()` because
    // of threading concerns.
    #[cfg(feature = "debug-logging")]
    {
        std::env::set_var("RUST_LOG", "warn");
        std::env::set_var("RUST_LOG_STYLE", "never");
        // Use try_init() because parallel workers may call _PG_init() multiple times
        let _ = env_logger::try_init();
    }

    if !pg_sys::process_shared_preload_libraries_in_progress {
        error!(
            "pg_search must be loaded via shared_preload_libraries. Add 'pg_search' to shared_preload_libraries in postgresql.conf and restart Postgres."
        );
    }

    // Register the DST assertion catalog for this process (a no-op outside `--features dst`)
    dst::init();

    postgres::options::init();
    postgres::build_logging::init();
    gucs::init();
    postgres::mutable_sweep::init();
    postgres::customscan::mpp::wakeup::init_wakeup_pool();
    postgres::customscan::joinscan::scan_state::init_base_session_state();
    postgres::customscan::mpp::worker::init_worker_base_session_state();
    // A term dictionary walk is one tantivy call that runs for seconds on a big segment, and
    // pg_search's own checks sit between segments.
    tantivy_common::interrupt::set_interrupt_check(check_for_interrupts);

    // RegisterCustomRmgr can only be called during shared_preload_libraries init. If pg_search
    // was loaded via plain CREATE EXTENSION (no preload), skip
    if pg_sys::process_shared_preload_libraries_in_progress {
        postgres::storage::custom_rmgr::register();
    }

    #[cfg(not(any(feature = "pg17", feature = "pg18")))]
    postgres::fake_aminsertcleanup::register();

    postgres::index_only::register_hook();

    #[allow(static_mut_refs)]
    #[allow(deprecated)]
    customscan::register_rel_pathlist(customscan::basescan::BaseScan);
    customscan::register_upper_path(customscan::aggregatescan::AggregateScan);
    customscan::register_upper_path(customscan::joinscan::JoinScan);

    // Register hook for SubPlan-based join opportunities (e.g. `col IN (SELECT ...) OR IS NULL`)
    // that PostgreSQL does not flatten into joins, so standard join planning does not consider them.
    customscan::register_subplan_join_pathlist();

    // Register hook for tracking explain statements (so planner warnings don't error under EXPLAIN)
    postgres::planner_warnings::register_explain_hook();

    // Register global planner hook for window function support and planner warning lifecycle
    customscan::register_planner_hook();

    // Initialize the filter query builder
    customscan::aggregatescan::filterquery::init_filter_query_builder();
}

#[pg_extern]
fn random_words(num_words: i32) -> String {
    use rand::RngExt;

    let mut rng = rand::rng();
    let letters = "abcdefghijklmnopqrstuvwxyz";
    let mut result = String::new();

    for _ in 0..num_words {
        // Choose a random word length between 3 and 7.
        let word_length = rng.random_range(3..=7);
        let mut word = String::new();

        for _ in 0..word_length {
            // Pick a random letter from the letters string.
            let random_index = rng.random_range(0..letters.len());
            // Safe to use .unwrap() because the index is guaranteed to be valid.
            let letter = letters.chars().nth(random_index).unwrap();
            word.push(letter);
        }
        result.push_str(&word);
        result.push(' ');
    }
    result.trim_end().to_string()
}

/// This module is required by `cargo pgrx test` invocations.
/// It must be visible at the root of your extension crate.
#[cfg(test)]
pub mod pg_test {
    pub fn setup(_options: Vec<&str>) {
        // perform one-off initialization when the pg_test framework starts
    }

    pub fn postgresql_conf_options() -> Vec<&'static str> {
        // return any postgresql.conf settings that are required for your tests
        vec![
            "shared_preload_libraries='pg_search'",
            "paradedb.mutable_sweep_interval=0",
            // `cargo test` runs `#[pg_test]` functions concurrently across threads, and several
            // tests (parallel index builds, MPP scans) spawn background workers simultaneously.
            // Raise the cluster-wide worker pool above Postgres's default of 8 so concurrent
            // `#[pg_test]` backends do not starve `LaunchParallelWorkers`.
            "max_worker_processes=64",
            "max_parallel_workers=64",
        ]
    }
}
