pub mod interrupt;
pub mod wakeup;

pub use interrupt::{
    HeldInterrupts, MainThread, MainThreadAbort, MainThreadExecutor, MainThreadToken,
    PgTokioRuntime, block_on_next, cancel_pending, catch_worker_panic, check_for_interrupts,
    check_for_interrupts_or_cancel, drop_on_main_thread, interrupted, run_on_main_thread,
    try_run_on_main_thread,
};

pub fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    use pgrx::pg_sys::panic::{CaughtError, ErrorReportWithLevel};

    if let Some(report) = payload.downcast_ref::<ErrorReportWithLevel>() {
        return report.message().to_string();
    }
    if let Some(caught) = payload.downcast_ref::<CaughtError>() {
        return match caught {
            CaughtError::RustPanic { ereport, .. } => ereport.message().to_string(),
            CaughtError::PostgresError(report) | CaughtError::ErrorReport(report) => {
                report.message().to_string()
            }
        };
    }
    if let Some(s) = payload.downcast_ref::<&str>() {
        return (*s).to_string();
    }
    if let Some(s) = payload.downcast_ref::<String>() {
        return s.clone();
    }
    "unknown panic payload".to_string()
}

pub mod glue {
    use pgrx::pg_sys;

    pub const MIN_TOTAL_WORKER_COUNT: u32 = 3;

    pub fn mpp_is_active() -> bool {
        producer_worker_cap() >= MIN_TOTAL_WORKER_COUNT - 1
    }

    pub fn query_allows_parallel_mode(root: &pg_sys::PlannerInfo) -> bool {
        !root.glob.is_null() && unsafe { (*root.glob).parallelModeOK }
    }

    pub fn producer_worker_cap() -> u32 {
        let max_per_gather = unsafe { pg_sys::max_parallel_workers_per_gather };
        let max_workers = unsafe { pg_sys::max_parallel_workers };
        max_per_gather.min(max_workers).max(0) as u32
    }

    #[derive(Default, Clone, Copy, Debug)]
    pub struct MppLaunchTiming {
        pub prepare_us: u64,
        pub plan_us: u64,
        pub attach_us: u64,
        pub leader_setup_us: u64,
        pub exec_us: u64,
        pub first_frame_us: u64,
        pub total_us: u64,
    }
}

#[macro_export]
macro_rules! mpp_log {
    ($($arg:tt)*) => {
        if $crate::gucs::mpp_debug() {
            pgrx::warning!($($arg)*);
        }
    };
}
