// Copyright (c) 2023-2026 ParadeDB, Inc.
//
// This file is part of ParadeDB - Postgres for Search and Analytics

pub use index_read::postgres::var::*;

use crate::api::FieldName;
use crate::postgres::customscan::pullup::{Access, ResTarget, ResolvedFastField, SourceContext};
use pgrx::is_a;
use pgrx::pg_sys::{self, CoerceViaIO, NodeTag::*, RelabelType};

/// Given a [`pg_sys::Node`] and a [`pg_sys::PlannerInfo`], attempt to find the [`pg_sys::Var`] and
/// the [`FieldName`] that it references.
///
/// It is the caller's responsibility to ensure that the node contains a Var with a valid FieldName.
pub unsafe fn find_one_resolved_var_and_fieldname(
    context: VarContext,
    source: &SourceContext,
    node: *mut pg_sys::Node,
    access: Access,
) -> (Option<ResolvedFastField>, Option<FieldName>) {
    if is_a(node, T_CoerceViaIO) {
        let expr = node.cast::<CoerceViaIO>();
        let arg = (*expr).arg;
        find_one_resolved_var_and_fieldname(context, source, arg as *mut pg_sys::Node, access)
    } else if is_a(node, T_RelabelType) {
        let relabel_type = node.cast::<RelabelType>();
        let arg = (*relabel_type).arg;
        find_one_resolved_var_and_fieldname(context, source, arg as *mut pg_sys::Node, access)
    } else if is_a(node, T_Var) {
        let (_, varattno) = context.var_relation(node.cast::<pg_sys::Var>());
        let name = source
            .tupdesc()
            .get(varattno as usize - 1)
            .map(|n| n.name().into());
        (
            source.resolve_fast_field(ResTarget::Attno(varattno), access),
            name,
        )
    } else if let Some(name) = unsafe { json_path_fieldname(context, node) } {
        let resolved = source.resolve_fast_field(ResTarget::Field(name.as_ref()), access);
        (resolved, Some(name))
    } else {
        (
            source.resolve_fast_field(ResTarget::Expr(node), access),
            None,
        )
    }
}
