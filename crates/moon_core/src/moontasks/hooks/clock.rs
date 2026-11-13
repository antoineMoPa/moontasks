//! When a schedule is due: the day it is on this machine's clock, and the minute of it.
//!
//! A schedule runs at most once a slot - a day, an hour, a minute, by how often it runs.
//! Missed - moon was closed at 08:30 - a daily one runs as soon as moon is open later that day,
//! and a day that has gone by with moon closed all of it is not made up for: after a week away,
//! opening moon runs today's, once, and none of the others. An hourly one is the same within
//! its hour.

use super::{Schedule, ScheduleEvery};

/// A moment on this machine's clock, as much of it as a schedule asks about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LocalNow {
    /// `2026-11-12`, which is what a schedule's claim on a day is named by.
    pub(super) date: String,
    /// Whether the day is a Saturday or a Sunday.
    pub(super) weekend: bool,
    pub(super) minute_of_day: u32,
}

impl LocalNow {
    /// Now, in this machine's zone. `localtime_r` rather than a date crate: the zone and its
    /// daylight-saving rules are the OS's to know.
    pub(super) fn read() -> Self {
        let now = crate::moontasks::store::now_unix() as libc::time_t;
        // SAFETY: zeroes are a valid `libc::tm`, which `localtime_r` writes over.
        let mut written: libc::tm = unsafe { std::mem::zeroed() };
        // SAFETY: both pointers are to locals that outlive the call.
        let placed = unsafe { libc::localtime_r(&now, &mut written) };
        assert!(
            !placed.is_null(),
            "localtime_r could not place {now} in this machine's zone"
        );
        Self {
            date: format!(
                "{:04}-{:02}-{:02}",
                written.tm_year + 1900,
                written.tm_mon + 1,
                written.tm_mday
            ),
            weekend: written.tm_wday == 0 || written.tm_wday == 6,
            minute_of_day: (written.tm_hour * 60 + written.tm_min) as u32,
        }
    }
}

/// The slot a schedule is due in now - `2026-11-12`, `2026-11-12T08`, `2026-11-12T08:30` -
/// or `None` while it is not due. Whether it has already run in that slot is the claim's to
/// say - see `runner`.
pub(super) fn due_slot_of(schedule: &Schedule, now: &LocalNow) -> Option<String> {
    let (hour, minute) = (now.minute_of_day / 60, now.minute_of_day % 60);
    match schedule.every {
        ScheduleEvery::Minute => Some(format!("{}T{hour:02}:{minute:02}", now.date)),
        ScheduleEvery::Hour => {
            (minute >= u32::from(schedule.at.minute)).then(|| format!("{}T{hour:02}", now.date))
        }
        ScheduleEvery::Day | ScheduleEvery::Weekdays => {
            let runs_today = schedule.every == ScheduleEvery::Day || !now.weekend;
            (runs_today && now.minute_of_day >= schedule.at.minute_of_day())
                .then(|| now.date.clone())
        }
    }
}
