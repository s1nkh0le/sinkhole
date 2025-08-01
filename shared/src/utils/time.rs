use time::format_description::FormatItem;
use time::macros::format_description;
use time::OffsetDateTime;

pub fn now_ns() -> u128 {
    // Monotonic-ish timestamp_ns (wall-clock ns since UNIX)
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

pub fn ns_to_hms(ns_since_epoch: u128) -> String {
    let signed_ns = i128::try_from(ns_since_epoch).map_err(|_| "timestamp out of range").unwrap(); // fixme

    let dt = OffsetDateTime::from_unix_timestamp_nanos(signed_ns).expect("ns out of range");

    static HMS: &[FormatItem<'_>] = format_description!("[hour]:[minute]:[second]");

    dt.format(HMS).unwrap()
}