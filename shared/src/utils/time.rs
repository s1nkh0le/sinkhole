use time::format_description::FormatItem;
use time::macros::format_description;
use time::OffsetDateTime;

/// Monotonic-ish timestamp_ns (wall-clock ns since UNIX)
pub fn now_ns() -> u64 {
    let n128 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    u64::try_from(n128).expect("nanosecond count overflowed u64 (won’t happen until ~2554)")
}

/// Conversion for human-displayable hour time
pub fn ns_to_hms(ns_since_epoch: u64) -> String {
    let signed_ns = i128::try_from(ns_since_epoch).expect("timestamp out of i128 range");

    let dt = OffsetDateTime::from_unix_timestamp_nanos(signed_ns).expect("ns out of range");

    static HMS: &[FormatItem<'_>] = format_description!("[hour]:[minute]:[second]");

    dt.format(HMS).unwrap()
}
