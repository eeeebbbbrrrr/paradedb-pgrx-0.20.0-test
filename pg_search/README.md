# pg_search

This README covers development of the `pg_search` extension. For installation, deployment, and usage, see the [ParadeDB documentation](https://www.paradedb.com/docs/start/introduction).

`pg_search` is supported on official PostgreSQL Global Development Group Postgres versions, starting at PostgreSQL 15.

## Crate Layout

The `pg_search` extension is split into 9 workspace crates (8 internal `rlib` crates plus the top-level `pg_search` `cdylib` crate) to enable parallel compilation across independent subsystems:

```text
                       +--------------------+
                       |       common       |
                       +---------+----------+
                                 |
                       +---------v----------+
                       |      storage       |
                       +---------+----------+
                                 |
                       +---------v----------+
                       |     index_read     |
                       +---------+----------+
                                 |
          +----------------------+----------------------+
          |                      |                      |
+---------v--------+   +---------v--------+   +---------v--------+
|   index_write    |   |       scan       |   |     planner      |
+---------+--------+   +---------+--------+   +---------+--------+
          |                      |                      |
          |             +--------+--------+             |
          |             |                 |             |
          |             |        +--------+-------------+
          |             |        |        |
          |    +--------v--------v-+    +-v-----------------+
          |    |     basescan      |    |     joinscan      |
          |    +--------+----------+    +---------+---------+
          |             |                         |
          +-------------+------------+------------+
                                     |
                            +--------v---------+
                            |    pg_search     |
                            |     (cdylib)     |
                            +------------------+
```

| Crate         | Depends On                                           | Responsibilities                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| :------------ | :--------------------------------------------------- | :------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `common`      | `tokenizers`, `dst`, `macros`                        | Root constants and macros (`impl_safe_drop`, `nodecast`, `debug2`), GUCs (`gucs.rs`), vector quantization primitives (`vector/*`), parallel worker registry (`parallel_worker/*`), vocabulary types (`FieldName`, `OrderByInfo`, `MvccVisibility`, `SearchFieldType`), and low-level Postgres wrappers (`postgres/{utils, catalog, node, locks, buffile, tuplesort, cost, types, types_arrow, pdb_owned_value, datetime, range, jsonb_support, composite}`). Named `common` rather than `core` because `pgrx` procedural macros expand `core::...` paths without a leading `::`. |
| `storage`     | `common`                                             | Postgres buffer-managed block storage (`postgres/storage/*`), index relation and reloption handling (`postgres/{rel, options, access, index, deparse, serializable, pgstat}.rs`), and `SearchIndexSchema` (`schema/mod.rs`).                                                                                                                                                                                                                                                                                                                                                     |
| `index_read`  | `common`, `storage`                                  | Query parsing and compilation (`query/*`), Tantivy index directory and MVCC snapshot readers (`index/directory/*`, `index/reader/*`, `index/writer/*`, `index/segment_pruning/*`, `index/ctid_map/*`, `index/stats/*`), heap visibility and tuple fetching (`postgres/heap.rs`), and shared parallel scan state (`postgres/parallel.rs`).                                                                                                                                                                                                                                        |
| `index_write` | `common`, `storage`, `index_read`                    | Index build, parallel build, vector IVF/k-d tree clustering (`superkmeans` is isolated here), segment merge policy, `INSERT`, `DELETE`, `VACUUM`, and background mutable segment sweeps.                                                                                                                                                                                                                                                                                                                                                                                         |
| `scan`        | `common`, `storage`, `index_read`                    | DataFusion physical execution operators (`scan/*`), custom aggregate and Top-K UDAFs (`postgres/customscan/datafusion/{topk_agg, numeric_agg, cardinality_agg, carry_value, timestamp_to_date, spill, memory}.rs`), deferred visibility filtering (`postgres/customscan/joinscan/{visibility_filter, range_partitioning_rule}.rs`), and DSM ring streams (`postgres/customscan/mpp/dsm_stream.rs`).                                                                                                                                                                              |
| `planner`     | `common`, `storage`, `index_read`                    | Tantivy-native aggregation execution (`aggregate/*`), Postgres planner inspection, qualifier extraction, path construction, projections (`score`, `snippet`, `window_agg`), and single-table Tantivy `AggregateScan` planning and execution.                                                                                                                                                                                                                                                                                                                                     |
| `basescan`    | `common`, `storage`, `index_read`, `scan`, `planner` | Single-table `BaseScan` custom scan planning, execution strategies (blockwise, tuplewise, Top-K, vector), scan state, and telemetry.                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| `joinscan`    | `common`, `storage`, `index_read`, `scan`, `planner` | Multi-table `JoinScan` and DataFusion `AggregateScan` planning and execution, logical/physical expression translation (`postgres/customscan/datafusion/*`, `pg_expr_udf.rs`), plan codecs (`scan/{codec, physical_codec, udf_codec}.rs`), and MPP distributed worker coordination (`postgres/customscan/mpp/*`).                                                                                                                                                                                                                                                                 |
| `pg_search`   | All 8 sub-crates                                     | Top-level `cdylib` extension crate: `_PG_init` bootstrap, planner hook registration (`postgres/customscan/hook.rs`), index AM handler and sequential/index-only scan callbacks (`postgres/{mod, scan, index_only, sequentialscan/*}.rs`), and SQL-exported `#[pg_extern]` / `#[pg_operator]` / `extension_sql!` definitions (`api/*`).                                                                                                                                                                                                                                           |

### Cross-Crate Conventions

- Module overlay re-exports: Each sub-crate re-exports its upstream crates' top-level modules (`postgres`, `index`, `query`, `scan`, `schema`, `aggregate`, `api`, `gucs`, `vector`, `parallel_worker`) and overlays its own submodules so internal `crate::...` paths remain consistent across the workspace.
- `pgrx` SQL entities: SQL-generating items (`#[pg_extern]`, `#[pg_operator]`, `#[pg_aggregate]`, `#[pg_cast]`, `#[derive(PostgresEnum)]`, `extension_sql!`) live in `pg_search` so the `cdylib` exports their C symbols and `.pgrxsc` / `__DATA,__pgrxsc` linker section entries. When a `#[derive(PostgresType)]` or `#[pg_extern]` must live in a sub-crate (for example due to Rust's orphan rules), its generated symbols must be referenced in `_SUBCRATE_PGRX_ANCHORS` in `pg_search/src/lib.rs` so the linker retains the object file from the `rlib` archive.

## Prerequisites

### Rust

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup install stable
```

### pgrx

The `cargo-pgrx` version must match the `pgrx` dependency declared in the [workspace `Cargo.toml`](../Cargo.toml). On Linux, `pgrx` also requires `libclang`, and [`.cargo/config.toml`](../.cargo/config.toml) links Linux builds with `lld`:

```bash
# Ubuntu
sudo apt install libclang-dev lld

# Arch Linux
sudo pacman -S extra/clang extra/lld
```

Then install `cargo-pgrx` and let it bootstrap a managed PostgreSQL installation under `~/.pgrx/`:

```bash
cargo install --locked cargo-pgrx --version 0.19.3
# On macOS, if `cargo pgrx init` fails with ICU-related errors, run
# `brew install icu4c`
# and then run
# `export PKG_CONFIG_PATH="/opt/homebrew/opt/icu4c@78/lib/pkgconfig:$PKG_CONFIG_PATH"`
# (the icu4c version may differ depending on what Homebrew has installed):
cargo pgrx init
```

`cargo pgrx init` builds every supported Postgres version this project targets (currently 15–18) into `~/.pgrx/<version>/pgrx-install/` and points future `cargo pgrx` commands at it — no system Postgres required. To target only a single version, pass e.g. `cargo pgrx init --pg18 download`.

### pgvector

`pg_search` uses `pgvector`'s types to index vector fields alongside text and other fields in ParadeDB indexes, so `pgvector` must be available before `pg_search` can be created. To build it against the pgrx-managed Postgres install (replace `18.6` with the version under `~/.pgrx/`):

```bash
git clone --branch v0.8.6 https://github.com/pgvector/pgvector.git
cd pgvector/

PG_CONFIG=~/.pgrx/18.6/pgrx-install/bin/pg_config make
PG_CONFIG=~/.pgrx/18.6/pgrx-install/bin/pg_config make install
```

## Running the Extension

`pg_search` must be loaded via `shared_preload_libraries`. Before running for the first time, add it to the pgrx-managed `postgresql.conf` (replace `18` with your target version):

```bash
echo "shared_preload_libraries = 'pg_search'" >> ~/.pgrx/data-18/postgresql.conf
```

Then start an interactive Postgres session with the extension built and loaded:

```bash
cargo pgrx run
```

Inside Postgres, create the extension (`CASCADE` also creates `pgvector`, which
`pg_search` requires):

```sql
CREATE EXTENSION pg_search CASCADE;
```

## Modifying the Extension

After making changes to the extension code:

1. Recompile and start Postgres:

   ```bash
   cargo pgrx run
   ```

2. Recreate the extension to load the latest changes:

   ```sql
   DROP EXTENSION pg_search;
   CREATE EXTENSION pg_search CASCADE;
   ```

3. **Schema Migrations:** If your changes introduce or modify extension SQL objects (functions, types, opclasses, etc.), you must provide an unreleased migration fragment in `pg_search/sql/unreleased/<PR_NUMBER>.<desc>.sql`. See [`CONTRIBUTING.md`](../CONTRIBUTING.md#pull-request-workflow) for details.

## Testing

Unit tests live across `pg_search` and its internal workspace crates (`common`, `storage`, `index_read`, `index_write`, `scan`, `planner`, `basescan`, `joinscan`) and run with:

```bash
cargo test -p <crate> -- a_specific_method_to_run
```

There are two kinds of unit tests in `pg_search`:

- **Pure Rust unit tests (`#[test]`):** Defined inside `#[cfg(test)] mod tests`. They run directly in the standalone test binary without starting Postgres, so they must not call code that transitively references Postgres (`pgrx` / `pg_sys`) symbols (such as GUC accessors, full session builders, or plan codecs). If a `#[test]` pulls in Postgres backend symbols (like `_BufferBlocks` or `_CurrentMemoryContext`), the test binary will fail to link on Linux or abort on startup on macOS with `dyld: symbol not found in flat namespace`. When this happens, either simplify the test to avoid the transitive `pg_sys` dependency or convert it to a `#[pg_test]`.
- **In-Postgres unit tests (`#[pg_test]`):** Run inside a Postgres backend as SQL functions and can use the full `pgrx` API. The annotation automatically reinstalls the extension — no manual install needed. Enclosing modules for `#[pg_test]` must be annotated with both `#[cfg(any(test, feature = "pg_test"))]` and `#[pgrx::pg_schema]` so `pgrx` exports the SQL wrapper when building the extension for tests.

For the other test categories (pg regress, integration tests, client property tests, stress tests, upgrade tests), see:

- [`pg_search/tests/pg_regress/README.md`](tests/pg_regress/README.md) — pg_regress tests
- [`tests/README.md`](../tests/README.md) — integration tests and client property tests
- [`stressgres/README.md`](../stressgres/README.md) — Stressgres, the stress-testing tool used locally and in CI
- [`CONTRIBUTING.md#testing`](../CONTRIBUTING.md#testing) — overview of all test categories and when to use which
- [`test-pg_search-upgrade/README.md`](../.github/actions/test-pg_search-upgrade/README.md) — compatibility tests for extension version upgrades

## Benchmarks

See [`benchmarks/README.md`](../benchmarks/README.md).
