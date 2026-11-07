pub use common::api::aggregate::*;

use crate::api::operator::estimate_matching_rows;
use crate::gucs;
use crate::index::reader::index::SearchIndexReader;
use crate::postgres::rel::PgSearchRelation;
use crate::query::SearchQueryInput;

pub trait MvccVisibilityExt {
    fn resolve_filtering_for_sources<'a>(
        &self,
        sources: impl IntoIterator<
            Item = (
                &'a PgSearchRelation,
                &'a SearchQueryInput,
                Option<&'a SearchIndexReader>,
            ),
        >,
    ) -> bool;

    fn resolve_filtering(
        &self,
        indexrel: &PgSearchRelation,
        query: &SearchQueryInput,
        reader: Option<&SearchIndexReader>,
    ) -> bool;
}

impl MvccVisibilityExt for MvccVisibility {
    fn resolve_filtering_for_sources<'a>(
        &self,
        sources: impl IntoIterator<
            Item = (
                &'a PgSearchRelation,
                &'a SearchQueryInput,
                Option<&'a SearchIndexReader>,
            ),
        >,
    ) -> bool {
        match self {
            MvccVisibility::Transaction => true,
            MvccVisibility::Raw => false,
            MvccVisibility::Threshold => {
                let mut largest = 0;
                for (indexrel, query, reader) in sources {
                    if query.has_heap_filters() || query.has_postgres_expressions() {
                        return true;
                    }
                    match estimate_matching_rows(indexrel, query.clone(), reader) {
                        Some(rows) => largest = largest.max(rows),
                        None => return true,
                    }
                }
                largest < gucs::visibility_threshold()
            }
        }
    }

    fn resolve_filtering(
        &self,
        indexrel: &PgSearchRelation,
        query: &SearchQueryInput,
        reader: Option<&SearchIndexReader>,
    ) -> bool {
        self.resolve_filtering_for_sources(std::iter::once((indexrel, query, reader)))
    }
}
