use crate::index::mvcc::SegmentView;
use crate::postgres::ParallelScanState;
use tantivy::index::SegmentId;

pub unsafe fn checkout_segment_for_source(
    pscan_state: *mut ParallelScanState,
    source_idx: usize,
) -> Option<SegmentId> {
    (*pscan_state).checkout_segment_for_source(source_idx)
}

pub unsafe fn segment_view(pscan_state: *mut ParallelScanState) -> SegmentView {
    (*pscan_state).segment_view()
}
