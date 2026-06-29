use crate::model::WorkStatus;

#[cfg(target_arch = "wasm32")]
fn counter(name: &str, delta: f64) -> Result<(), String> {
    patina_sdk::toys::measure::counter(name, delta)
}

#[cfg(not(target_arch = "wasm32"))]
fn counter(name: &str, delta: f64) -> Result<(), String> {
    assert!(!name.trim().is_empty(), "metric name must not be empty");
    assert!(delta.is_finite(), "metric delta must be finite");
    Ok(())
}

#[cfg(target_arch = "wasm32")]
fn gauge(name: &str, value: f64) -> Result<(), String> {
    patina_sdk::toys::measure::gauge(name, value)
}

#[cfg(not(target_arch = "wasm32"))]
fn gauge(name: &str, value: f64) -> Result<(), String> {
    assert!(!name.trim().is_empty(), "metric name must not be empty");
    assert!(value.is_finite(), "metric value must be finite");
    Ok(())
}

pub(crate) fn record_dispatch_call() -> Result<(), String> {
    counter("slate_dispatch_calls", 1.0)
}

pub(crate) fn record_dispatch_command() -> Result<(), String> {
    counter("slate_dispatch_command_total", 1.0)
}

pub(crate) fn record_operation(operation: SlateOperation) -> Result<(), String> {
    counter(operation.metric_name(), 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SlateOperation {
    CheckWork,
    PacketWork,
}

impl SlateOperation {
    fn metric_name(self) -> &'static str {
        match self {
            Self::CheckWork => "slate_operation_check_work_total",
            Self::PacketWork => "slate_operation_packet_work_total",
        }
    }
}

pub(crate) fn record_transition(
    event_type: &str,
    from: WorkStatus,
    to: WorkStatus,
) -> Result<(), String> {
    match event_type {
        "promoted" => counter("slate_transition_promoted_total", 1.0)?,
        "activated" => counter("slate_transition_activated_total", 1.0)?,
        other => {
            return Err(format!(
                "cannot emit Slate transition metric for undeclared event type '{other}'"
            ))
        }
    }
    counter("slate_transition_total", 1.0)?;
    gauge("slate_last_transition_from_status", status_code(from))?;
    gauge("slate_last_transition_to_status", status_code(to))
}

fn status_code(status: WorkStatus) -> f64 {
    match status {
        WorkStatus::Draft => 1.0,
        WorkStatus::Ready => 2.0,
        WorkStatus::Active => 3.0,
        WorkStatus::Blocked => 4.0,
        WorkStatus::Paused => 5.0,
        WorkStatus::Complete | WorkStatus::Completed | WorkStatus::Done => 6.0,
        WorkStatus::Abandoned => 7.0,
    }
}

pub(crate) fn record_check(total: usize, checked: usize, passed: bool) -> Result<(), String> {
    record_operation(SlateOperation::CheckWork)?;
    gauge("slate_last_check_proof_total", total as f64)?;
    gauge("slate_last_check_proof_checked", checked as f64)?;
    counter(
        if passed {
            "slate_check_passed_total"
        } else {
            "slate_check_failed_total"
        },
        1.0,
    )
}

pub(crate) fn record_packet(
    total: usize,
    checked: usize,
    cleanup_count: usize,
) -> Result<(), String> {
    record_operation(SlateOperation::PacketWork)?;
    gauge("slate_last_packet_proof_total", total as f64)?;
    gauge("slate_last_packet_proof_checked", checked as f64)?;
    gauge("slate_last_packet_cleanup_candidates", cleanup_count as f64)
}
