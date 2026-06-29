use crate::model::WorkStatus;

fn metric_suffix(value: &str) -> String {
    value
        .chars()
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            'A'..='Z' => c.to_ascii_lowercase(),
            _ => '_',
        })
        .collect()
}

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

pub(crate) fn record_operation(operation: &str) -> Result<(), String> {
    counter(
        &format!("slate_operation_{}_total", metric_suffix(operation)),
        1.0,
    )
}

pub(crate) fn record_transition(
    event_type: &str,
    from: WorkStatus,
    to: WorkStatus,
) -> Result<(), String> {
    record_operation(event_type)?;
    counter(
        &format!(
            "slate_transition_{}_to_{}_total",
            metric_suffix(from.as_str()),
            metric_suffix(to.as_str())
        ),
        1.0,
    )
}

pub(crate) fn record_check(total: usize, checked: usize, passed: bool) -> Result<(), String> {
    record_operation("check_work")?;
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
    record_operation("packet_work")?;
    gauge("slate_last_packet_proof_total", total as f64)?;
    gauge("slate_last_packet_proof_checked", checked as f64)?;
    gauge("slate_last_packet_cleanup_candidates", cleanup_count as f64)
}
