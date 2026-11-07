// Copyright (c) 2023-2026 ParadeDB, Inc.
//
// This file is part of ParadeDB - Postgres for Search and Analytics

use pgrx::pg_sys;

pub unsafe fn find_final_rel(root: *mut pg_sys::PlannerInfo) -> *mut pg_sys::RelOptInfo {
    let mut final_rel = std::ptr::null_mut();

    let join_rels = pgrx::PgList::<pg_sys::RelOptInfo>::from_pg((*root).join_rel_list);
    let all_baserels = (*root).all_baserels;
    for rel in join_rels.iter_ptr() {
        if pgrx::pg_sys::bms_is_subset(all_baserels, (*rel).relids) {
            final_rel = rel;
            break;
        }
    }

    if final_rel.is_null() && (*root).simple_rel_array_size > 1 {
        for i in 1..(*root).simple_rel_array_size {
            let rel = *(*root).simple_rel_array.add(i as usize);
            if !rel.is_null() && (*rel).reloptkind == pg_sys::RelOptKind::RELOPT_BASEREL {
                final_rel = rel;
                break;
            }
        }
    }

    final_rel
}
