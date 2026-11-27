//! Time, in the shapes ravel actually uses — in-tree since leg D
//! (2026-11-27) replaced chrono.
//!
//! Chrono's tree (~22 packages: iana-time-zone, num-traits, the
//! windows-core/android/wasm platform stacks) could not fit the
//! ashfall budget, and the surface ravel uses is small and closed:
//! parse an RFC 3339 string, format an instant in UTC or in this
//! machine's local time, add a duration, compare. That is all here.
//!
//! Semantics are matched to the chrono calls they replaced:
//! - `Instant::parse_rfc3339` reads what `DateTime::parse_from_rfc3339`
//!   read: `YYYY-MM-DDTHH:MM:SS[.fff]` with `Z` or a `±HH:MM` offset
//!   (seconds may be `:60` — a leap second — clamped to 59, as chrono
//!   does). Garbage is `None`, never a guess.
//! - `to_rfc3339` renders `+00:00` with chrono's default subsecond
//!   rule (no fraction when whole; otherwise the fraction in groups
//!   of three digits, trailing zero-groups dropped).
//! - Local time goes through libc `localtime_r`, so DST and the
//!   machine's zone are the C library's, exactly as chrono's Local
//!   did; the UTC offset shown by `%:z` is `tm_gmtoff`.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A moment: seconds and nanoseconds since the Unix epoch (UTC).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Instant {
    secs: i64,
    nanos: u32,
}

impl Instant {
    pub fn now() -> Instant {
        let d = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::ZERO);
        Instant {
            secs: d.as_secs() as i64,
            nanos: d.subsec_nanos(),
        }
    }

    pub fn from_unix(secs: i64, nanos: u32) -> Instant {
        Instant { secs, nanos }
    }

    /// Read an RFC 3339 timestamp; `None` when it is not one.
    pub fn parse_rfc3339(s: &str) -> Option<Instant> {
        let b = s.as_bytes();
        if b.len() < 20 {
            return None;
        }
        let num = |i: usize, n: usize| -> Option<i64> {
            let mut v: i64 = 0;
            for k in 0..n {
                let c = *b.get(i + k)?;
                if !c.is_ascii_digit() {
                    return None;
                }
                v = v * 10 + (c - b'0') as i64;
            }
            Some(v)
        };
        let (year, mon, day) = (num(0, 4)?, num(5, 2)?, num(8, 2)?);
        if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') {
            return None;
        }
        if !matches!(b.get(10), Some(b'T') | Some(b't') | Some(b' ')) {
            return None;
        }
        let (hh, mm) = (num(11, 2)?, num(14, 2)?);
        if b.get(13) != Some(&b':') || b.get(16) != Some(&b':') {
            return None;
        }
        let mut ss = num(17, 2)?;
        if ss == 60 {
            ss = 59; // a leap second: chrono clamps it the same way
        }
        if !(1..=12).contains(&mon) || day < 1 || day > 31 || hh > 23 || mm > 59 || ss > 59 {
            return None;
        }
        let mut i = 19;
        let mut nanos: u32 = 0;
        if b.get(i) == Some(&b'.') {
            i += 1;
            let mut digits = 0;
            let mut frac: u64 = 0;
            while let Some(c) = b.get(i) {
                if !c.is_ascii_digit() {
                    break;
                }
                if digits < 9 {
                    frac = frac * 10 + (c - b'0') as u64;
                    digits += 1;
                }
                i += 1;
            }
            if digits == 0 {
                return None;
            }
            while digits < 9 {
                frac *= 10;
                digits += 1;
            }
            nanos = frac as u32;
        }
        let offset: i64 = match b.get(i) {
            Some(b'Z') | Some(b'z') => {
                i += 1;
                0
            }
            Some(sign @ (b'+' | b'-')) => {
                let (oh, om) = (num(i + 1, 2)?, num(i + 4, 2)?);
                if b.get(i + 3) != Some(&b':') || oh > 23 || om > 59 {
                    return None;
                }
                i += 6;
                let v = oh * 3600 + om * 60;
                if *sign == b'-' {
                    -v
                } else {
                    v
                }
            }
            _ => return None,
        };
        if i != b.len() {
            return None;
        }
        let days = days_from_civil(year, mon, day);
        Some(Instant {
            secs: days * 86400 + hh * 3600 + mm * 60 + ss - offset,
            nanos,
        })
    }

    pub fn timestamp(self) -> i64 {
        self.secs
    }

    pub fn timestamp_millis(self) -> i64 {
        self.secs * 1000 + (self.nanos / 1_000_000) as i64
    }

    /// Chrono's `to_rfc3339`: `+00:00`, subseconds in groups of three.
    pub fn to_rfc3339(self) -> String {
        let p = utc_parts(self.secs);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}+00:00",
            p.year,
            p.mon,
            p.day,
            p.hour,
            p.min,
            p.sec,
            fraction(self.nanos, Frac::Auto)
        )
    }

    /// `to_rfc3339_opts(SecondsFormat::Secs, true)`: whole seconds, `Z`.
    pub fn to_rfc3339_secs_z(self) -> String {
        let p = utc_parts(self.secs);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            p.year, p.mon, p.day, p.hour, p.min, p.sec
        )
    }

    /// `to_rfc3339_opts(SecondsFormat::Millis, true)`: milliseconds, `Z`.
    pub fn to_rfc3339_millis_z(self) -> String {
        let p = utc_parts(self.secs);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            p.year,
            p.mon,
            p.day,
            p.hour,
            p.min,
            p.sec,
            self.nanos / 1_000_000
        )
    }

    /// This instant in local time, `fmt` as in [`format_parts`].
    pub fn format_local(self, fmt: &str) -> String {
        let p = local_parts(self.secs);
        format_parts(&p, fmt)
    }

    /// `DateTime<Local>::to_rfc3339` for this instant.
    pub fn to_rfc3339_local(self) -> String {
        let p = local_parts(self.secs);
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}{}",
            p.year,
            p.mon,
            p.day,
            p.hour,
            p.min,
            p.sec,
            fraction(self.nanos, Frac::Auto),
            colon_offset(p.offset_secs)
        )
    }
}

impl std::ops::Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, d: Duration) -> Instant {
        Instant {
            secs: self.secs + d.as_secs() as i64,
            nanos: self.nanos,
        }
    }
}

impl std::ops::Sub<Duration> for Instant {
    type Output = Instant;
    fn sub(self, d: Duration) -> Instant {
        Instant {
            secs: self.secs - d.as_secs() as i64,
            nanos: self.nanos,
        }
    }
}

impl std::ops::Sub<Instant> for Instant {
    type Output = Duration;
    fn sub(self, earlier: Instant) -> Duration {
        let secs = self.secs - earlier.secs;
        if secs < 0 {
            return Duration::ZERO;
        }
        let secs = if self.nanos < earlier.nanos {
            secs - 1
        } else {
            secs
        };
        Duration::from_secs(secs.max(0) as u64)
    }
}

enum Frac {
    Auto,
}

/// Chrono's default fraction: none, or 3/6/9 digits as needed.
fn fraction(nanos: u32, _f: Frac) -> String {
    if nanos == 0 {
        return String::new();
    }
    let nine = format!("{nanos:09}");
    let keep = if nanos % 1_000_000 == 0 {
        3
    } else if nanos % 1_000 == 0 {
        6
    } else {
        9
    };
    format!(".{}", &nine[..keep])
}

fn colon_offset(offset_secs: i64) -> String {
    let sign = if offset_secs < 0 { "-" } else { "+" };
    let a = offset_secs.abs();
    format!("{sign}{:02}:{:02}", a / 3600, (a % 3600) / 60)
}

/// Broken-down calendar fields for one instant.
#[derive(Debug, Clone, Copy)]
pub struct Parts {
    pub year: i64,
    pub mon: i64,
    pub day: i64,
    pub hour: i64,
    pub min: i64,
    pub sec: i64,
    /// Seconds east of UTC (0 for the UTC breakdown).
    pub offset_secs: i64,
}

/// The UTC breakdown, by Howard Hinnant's civil-from-days algorithm.
pub fn utc_parts(secs: i64) -> Parts {
    let days = secs.div_euclid(86400);
    let tod = secs.rem_euclid(86400);
    let (year, mon, day) = civil_from_days(days);
    Parts {
        year,
        mon,
        day,
        hour: tod / 3600,
        min: (tod % 3600) / 60,
        sec: tod % 60,
        offset_secs: 0,
    }
}

/// The local breakdown, from libc `localtime_r`.
pub fn local_parts(secs: i64) -> Parts {
    // SAFETY: `localtime_r` writes into the caller-owned `tm` and
    // returns null only on overflow; on null we fall back to UTC
    // rather than read unwritten fields.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let t = secs as libc::time_t;
        if libc::localtime_r(&t, &mut tm).is_null() {
            return utc_parts(secs);
        }
        Parts {
            year: tm.tm_year as i64 + 1900,
            mon: tm.tm_mon as i64 + 1,
            day: tm.tm_mday as i64,
            hour: tm.tm_hour as i64,
            min: tm.tm_min as i64,
            sec: tm.tm_sec as i64,
            offset_secs: tm.tm_gmtoff as i64,
        }
    }
}

/// Format with the strftime subset ravel uses: `%Y %m %d %H %M %S
/// %:z %z %%`. Anything else passes through untouched.
pub fn format_parts(p: &Parts, fmt: &str) -> String {
    let mut out = String::new();
    let b = fmt.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 1 < b.len() {
            if b[i + 1] == b':' && b.get(i + 2) == Some(&b'z') {
                out.push_str(&colon_offset(p.offset_secs));
                i += 3;
                continue;
            }
            match b[i + 1] {
                b'Y' => out.push_str(&format!("{:04}", p.year)),
                b'm' => out.push_str(&format!("{:02}", p.mon)),
                b'd' => out.push_str(&format!("{:02}", p.day)),
                b'H' => out.push_str(&format!("{:02}", p.hour)),
                b'M' => out.push_str(&format!("{:02}", p.min)),
                b'S' => out.push_str(&format!("{:02}", p.sec)),
                b'z' => {
                    let sign = if p.offset_secs < 0 { "-" } else { "+" };
                    let a = p.offset_secs.abs();
                    out.push_str(&format!("{sign}{:02}{:02}", a / 3600, (a % 3600) / 60));
                }
                b'%' => out.push('%'),
                other => {
                    out.push('%');
                    out.push(other as char);
                }
            }
            i += 2;
        } else {
            // fmt strings here are ASCII; copy byte-wise as chars.
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

/// Days since the epoch for a civil date (Hinnant's algorithm).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// The civil date for days since the epoch (Hinnant's algorithm).
pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// -- The call shapes the rest of the tree uses -----------------------

/// `Utc::now().to_rfc3339()`.
pub fn now_rfc3339() -> String {
    Instant::now().to_rfc3339()
}

/// `Local::now().to_rfc3339()`.
pub fn now_local_rfc3339() -> String {
    Instant::now().to_rfc3339_local()
}

/// `Local::now().format(fmt)`.
pub fn now_local_format(fmt: &str) -> String {
    Instant::now().format_local(fmt)
}

/// Parse an RFC 3339 string and format it in local time; `None` when
/// it does not parse (callers fall back to the raw string, as the
/// chrono `.unwrap_or_else` chains did).
pub fn parse_local_format(ts: &str, fmt: &str) -> Option<String> {
    Instant::parse_rfc3339(ts).map(|t| t.format_local(fmt))
}

/// The Unix seconds of an RFC 3339 string.
pub fn unix(ts: &str) -> Option<i64> {
    Instant::parse_rfc3339(ts).map(|t| t.timestamp())
}

/// The Unix milliseconds of an RFC 3339 string.
pub fn unix_millis(ts: &str) -> Option<i64> {
    Instant::parse_rfc3339(ts).map(|t| t.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_instants_round_trip() {
        // The epoch, and 2026-11-27T03:19:00Z = 1795749540.
        assert_eq!(Instant::parse_rfc3339("1970-01-01T00:00:00Z").unwrap().timestamp(), 0);
        let t = Instant::parse_rfc3339("2026-11-27T03:19:00Z").unwrap();
        assert_eq!(t.timestamp(), 1795749540);
        assert_eq!(t.to_rfc3339(), "2026-11-27T03:19:00+00:00");
        assert_eq!(t.to_rfc3339_secs_z(), "2026-11-27T03:19:00Z");
        // Offsets normalise to the same instant.
        assert_eq!(
            Instant::parse_rfc3339("2026-11-26T21:19:00-06:00").unwrap(),
            t
        );
        // Fractions: groups of three, trailing groups dropped.
        let f = Instant::parse_rfc3339("2026-11-27T03:19:00.123456Z").unwrap();
        assert_eq!(f.to_rfc3339(), "2026-11-27T03:19:00.123456+00:00");
        assert_eq!(f.timestamp_millis(), 1795749540123);
        let f = Instant::parse_rfc3339("2026-11-27T03:19:00.500Z").unwrap();
        assert_eq!(f.to_rfc3339(), "2026-11-27T03:19:00.500+00:00");
        // Leap second clamps; garbage refuses.
        assert!(Instant::parse_rfc3339("2026-11-27T03:19:60Z").is_some());
        assert!(Instant::parse_rfc3339("not a time").is_none());
        assert!(Instant::parse_rfc3339("2026-13-27T03:19:00Z").is_none());
        assert!(Instant::parse_rfc3339("2026-11-27T03:19:00").is_none());
        // Civil conversions agree with the epoch anchors.
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20784), (2026, 11, 27));
    }
}
