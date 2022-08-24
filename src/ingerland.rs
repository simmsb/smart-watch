// The contents of this module is mostly a const-ified adaption of https://github.com/Rapptz/eos

use eos::gregorian::{date_to_epoch_days, days_in_month, weekday_difference, weekday_from_days};
use eos::{DateTime, TimeZone, UtcOffset};

struct StaticTimeZoneInner {
    transitions: &'static [Transition],
    ttypes: &'static [TransitionType],
    posix: &'static PosixTimeZone,
}

#[derive(Clone, Copy)]
pub struct StaticTimeZone {
    inner: &'static StaticTimeZoneInner,
}

impl StaticTimeZone {
    fn get_transition(&self, ts: NaiveTimestamp) -> Option<&Transition> {
        let idx = match self
            .inner
            .transitions
            .binary_search_by_key(&ts, |trans| trans.utc_start)
        {
            Ok(idx) => idx,
            Err(idx) => {
                if idx != self.inner.transitions.len() {
                    idx - 1
                } else {
                    return None;
                }
            }
        };
        self.inner.transitions.get(idx)
    }
}

impl eos::TimeZone for StaticTimeZone {
    fn name(&self, ts: eos::Timestamp) -> Option<&str> {
        match self.get_transition(ts.into()) {
            None => self.inner.posix.name(ts),
            Some(trans) => self
                .inner
                .ttypes
                .get(trans.name_idx)
                .map(|ttype| ttype.abbr),
        }
    }

    fn offset(&self, ts: eos::Timestamp) -> UtcOffset {
        match self.get_transition(ts.into()) {
            None => self.inner.posix.offset(ts),
            Some(trans) => trans.offset,
        }
    }

    fn convert_utc(self, mut utc: eos::DateTime<eos::Utc>) -> eos::DateTime<Self>
    where
        Self: Sized,
    {
        let ts = utc.timestamp();

        match self.get_transition(ts.into()) {
            None => {
                self.inner.posix.shift_utc(&mut utc);
                utc.with_timezone(self)
            }
            Some(trans) => {
                utc.shift(trans.offset);
                utc.with_timezone(self)
            }
        }
    }

    fn resolve(self, date: eos::Date, time: eos::Time) -> eos::DateTimeResolution<Self>
    where
        Self: Sized,
    {
        let ts = NaiveTimestamp::new(&date, &time);

        let (prev, trans, next) = match self
            .inner
            .transitions
            .binary_search_by_key(&ts, |t| t.start)
        {
            Ok(idx) => (
                self.inner.transitions.get(idx.wrapping_sub(1)),
                &self.inner.transitions[idx],
                self.inner.transitions.get(idx + 1),
            ),
            Err(idx) if idx != self.inner.transitions.len() => (
                self.inner.transitions.get(idx.wrapping_sub(1)),
                &self.inner.transitions[idx - 1],
                Some(&self.inner.transitions[idx]),
            ),
            Err(idx) => {
                if !self.inner.transitions.is_empty() {
                    let trans = &self.inner.transitions[idx - 1];
                    if trans.is_missing(ts) {
                        let earlier = self.inner.transitions[idx - 2].offset;
                        return eos::DateTimeResolution::missing(
                            date,
                            time,
                            earlier,
                            trans.offset,
                            self,
                        );
                    }
                }

                let (kind, earlier, later) = self.inner.posix.partial_resolution(&date, &time);
                return match kind {
                    eos::DateTimeResolutionKind::Missing => {
                        eos::DateTimeResolution::missing(date, time, earlier, later, self.clone())
                    }
                    eos::DateTimeResolutionKind::Unambiguous => {
                        eos::DateTimeResolution::unambiguous(date, time, earlier, self.clone())
                    }
                    eos::DateTimeResolutionKind::Ambiguous => {
                        eos::DateTimeResolution::ambiguous(date, time, earlier, later, self.clone())
                    }
                };
            }
        };

        if let Some(next) = next {
            if next.is_ambiguous(ts) {
                return eos::DateTimeResolution::ambiguous(
                    date,
                    time,
                    trans.offset,
                    next.offset,
                    self,
                );
            }
        }

        if trans.is_missing(ts) {
            if let Some(prev) = prev {
                return eos::DateTimeResolution::missing(
                    date,
                    time,
                    prev.offset,
                    trans.offset,
                    self,
                );
            }
        }

        eos::DateTimeResolution::unambiguous(date, time, trans.offset, self)
    }

    fn is_fixed(&self) -> bool {
        false
    }
}

struct Transition {
    name_idx: usize,
    start: NaiveTimestamp,
    utc_start: NaiveTimestamp,
    end: NaiveTimestamp,
    offset: UtcOffset,
}

struct TransitionType {
    offset: i32,
    is_dst: bool,
    abbr: &'static str,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
struct NaiveTimestamp(i64);

impl<Tz> From<DateTime<Tz>> for NaiveTimestamp
where
    Tz: TimeZone,
{
    fn from(dt: DateTime<Tz>) -> Self {
        let ts = dt.days_since_epoch() as i64 * 86400
            + dt.hour() as i64 * 3600
            + dt.minute() as i64 * 60
            + dt.second() as i64;

        Self(ts)
    }
}

impl From<eos::Timestamp> for NaiveTimestamp {
    fn from(ts: eos::Timestamp) -> Self {
        Self(ts.as_seconds())
    }
}

impl NaiveTimestamp {
    fn new(date: &eos::Date, time: &eos::Time) -> Self {
        let ts = date.days_since_epoch() as i64 * 86400
            + time.hour() as i64 * 3600
            + time.minute() as i64 * 60
            + time.second() as i64;

        Self(ts)
    }

    fn into_inner(self) -> i64 {
        self.0
    }

    fn from_seconds(secs: i64) -> Self {
        Self(secs)
    }

    fn to_regular(self, offset: &eos::UtcOffset) -> eos::Timestamp {
        eos::Timestamp::from_seconds(self.0 - offset.total_seconds() as i64)
    }
}

#[derive(Clone)]
struct PosixTimeZone {
    std_abbr: &'static str,
    std_offset: UtcOffset,
    dst: Option<&'static DstTransitionInfo>,
}

impl PosixTimeZone {
    fn shift_utc(&self, utc: &mut eos::DateTime<eos::Utc>) {
        let ts = NaiveTimestamp::new(utc.date(), utc.time());
        match self.dst.as_ref() {
            None => {
                utc.shift(self.std_offset);
            }
            Some(dst) => {
                let mut dst_on = dst.start.timestamp_in_year(utc.year());
                let mut dst_off = dst.end.timestamp_in_year(utc.year());
                dst_on.0 -= self.std_offset.total_seconds() as i64;
                dst_off.0 -= dst.offset.total_seconds() as i64;

                let is_dst = if dst_on < dst_off {
                    dst_on <= ts && ts < dst_off
                } else {
                    !(dst_off <= ts && ts < dst_on)
                };
                if is_dst {
                    utc.shift(dst.offset);
                } else {
                    utc.shift(self.std_offset);
                }
            }
        }
    }

    fn partial_resolution(
        &self,
        date: &eos::Date,
        time: &eos::Time,
    ) -> (eos::DateTimeResolutionKind, UtcOffset, UtcOffset) {
        match &self.dst {
            Some(dst) => {
                let ts = NaiveTimestamp::new(date, time).into_inner();
                let dst_diff = dst.base_offset.total_seconds() as i64;
                let end = dst.end.timestamp_in_year(date.year()).into_inner();
                let start = dst.start.timestamp_in_year(date.year()).into_inner();
                let is_dst = if start < end {
                    start <= ts && ts < end
                } else {
                    !(end <= ts && ts < start)
                };
                if dst_diff > 0 {
                    if (end - dst_diff) <= ts && ts < end {
                        (
                            eos::DateTimeResolutionKind::Ambiguous,
                            dst.offset,
                            self.std_offset,
                        )
                    } else if start <= ts && ts < (start + dst_diff) {
                        (
                            eos::DateTimeResolutionKind::Missing,
                            self.std_offset,
                            dst.offset,
                        )
                    } else if is_dst {
                        (
                            eos::DateTimeResolutionKind::Unambiguous,
                            dst.offset,
                            dst.offset,
                        )
                    } else {
                        (
                            eos::DateTimeResolutionKind::Unambiguous,
                            self.std_offset,
                            self.std_offset,
                        )
                    }
                } else {
                    if (start + dst_diff) <= ts && ts < start {
                        (
                            eos::DateTimeResolutionKind::Ambiguous,
                            self.std_offset,
                            dst.offset,
                        )
                    } else if end <= ts && ts < (end - dst_diff) {
                        (
                            eos::DateTimeResolutionKind::Missing,
                            dst.offset,
                            self.std_offset,
                        )
                    } else if is_dst {
                        (
                            eos::DateTimeResolutionKind::Unambiguous,
                            dst.offset,
                            dst.offset,
                        )
                    } else {
                        (
                            eos::DateTimeResolutionKind::Unambiguous,
                            self.std_offset,
                            self.std_offset,
                        )
                    }
                }
            }
            None => (
                eos::DateTimeResolutionKind::Unambiguous,
                self.std_offset,
                self.std_offset,
            ),
        }
    }
}

impl TimeZone for PosixTimeZone {
    fn name(&self, ts: eos::Timestamp) -> Option<&str> {
        match &self.dst {
            Some(dst) => {
                if dst.is_dst_utc(ts, &self.std_offset) {
                    Some(dst.abbr)
                } else {
                    Some(self.std_abbr)
                }
            }
            None => Some(self.std_abbr),
        }
    }

    fn offset(&self, ts: eos::Timestamp) -> UtcOffset {
        match &self.dst {
            Some(dst) => {
                if dst.is_dst_utc(ts, &self.std_offset) {
                    dst.offset
                } else {
                    self.std_offset
                }
            }
            None => self.std_offset,
        }
    }

    fn resolve(self, date: eos::Date, time: eos::Time) -> eos::DateTimeResolution<Self>
    where
        Self: Sized,
    {
        let (kind, earlier, later) = self.partial_resolution(&date, &time);
        match kind {
            eos::DateTimeResolutionKind::Missing => {
                eos::DateTimeResolution::missing(date, time, earlier, later, self)
            }
            eos::DateTimeResolutionKind::Unambiguous => {
                eos::DateTimeResolution::unambiguous(date, time, earlier, self)
            }
            eos::DateTimeResolutionKind::Ambiguous => {
                eos::DateTimeResolution::ambiguous(date, time, earlier, later, self)
            }
        }
    }

    fn convert_utc(self, mut utc: DateTime<eos::Utc>) -> DateTime<Self>
    where
        Self: Sized,
    {
        self.shift_utc(&mut utc);
        utc.with_timezone(self)
    }

    fn is_fixed(&self) -> bool {
        self.dst.is_none()
    }
}

struct DstTransitionInfo {
    abbr: &'static str,
    offset: UtcOffset,
    start: DstTransitionRule,
    end: DstTransitionRule,
    base_offset: UtcOffset,
}

impl DstTransitionInfo {
    fn is_active(&self, date: &eos::Date, time: &eos::Time) -> bool {
        let ts = NaiveTimestamp::new(date, time);
        let start = self.start.timestamp_in_year(date.year());
        let end = self.end.timestamp_in_year(date.year());
        if start < end {
            start <= ts && ts < end
        } else {
            !(end <= ts && ts < start)
        }
    }

    fn is_dst_utc(&self, ts: eos::Timestamp, std_offset: &UtcOffset) -> bool {
        let utc = ts.to_utc();
        let start = self
            .start
            .timestamp_in_year(utc.year())
            .to_regular(std_offset);
        let end = self
            .end
            .timestamp_in_year(utc.year())
            .to_regular(&self.offset);
        if start < end {
            start <= ts && ts < end
        } else {
            !(end <= ts && ts < start)
        }
    }
}

struct DstTransitionRule {
    month: u8,
    n: u8,
    weekday: u8,
    offset: i64,
}

impl DstTransitionRule {
    fn timestamp_in_year(&self, year: i16) -> NaiveTimestamp {
        match self {
            Self {
                month,
                n,
                weekday,
                offset,
            } => {
                let first_weekday = weekday_from_days(date_to_epoch_days(year, *month, 1));
                let days_in_month = days_in_month(year, *month);
                let mut day = weekday_difference(*weekday, first_weekday) + 1 + (n - 1) * 7;
                if day > days_in_month {
                    day -= 7;
                }
                let epoch = date_to_epoch_days(year, *month, day) as i64;
                let seconds = epoch * 86400 + offset;
                NaiveTimestamp::from_seconds(seconds)
            }
        }
    }
}

pub static INGERLAND: StaticTimeZone = StaticTimeZone {
    inner: &StaticTimeZoneInner {
        transitions: &[
            Transition {
                name_idx: 0,
                start: NaiveTimestamp(-9223372036854775808),
                utc_start: NaiveTimestamp(-9223372036854775808),
                end: NaiveTimestamp(-9223372036854775808),
                offset: UtcOffset::from_hms(0, -1, -15).unwrap(),
            },
            Transition {
                name_idx: 4,
                start: NaiveTimestamp(-3852662400),
                utc_start: NaiveTimestamp(-3852662325),
                end: NaiveTimestamp(-3852662325),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1691964000),
                utc_start: NaiveTimestamp(-1691964000),
                end: NaiveTimestamp(-1691960400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1680469200),
                utc_start: NaiveTimestamp(-1680472800),
                end: NaiveTimestamp(-1680472800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1664143200),
                utc_start: NaiveTimestamp(-1664143200),
                end: NaiveTimestamp(-1664139600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1650142800),
                utc_start: NaiveTimestamp(-1650146400),
                end: NaiveTimestamp(-1650146400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1633903200),
                utc_start: NaiveTimestamp(-1633903200),
                end: NaiveTimestamp(-1633899600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1617483600),
                utc_start: NaiveTimestamp(-1617487200),
                end: NaiveTimestamp(-1617487200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1601848800),
                utc_start: NaiveTimestamp(-1601848800),
                end: NaiveTimestamp(-1601845200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1586034000),
                utc_start: NaiveTimestamp(-1586037600),
                end: NaiveTimestamp(-1586037600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1570399200),
                utc_start: NaiveTimestamp(-1570399200),
                end: NaiveTimestamp(-1570395600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1552165200),
                utc_start: NaiveTimestamp(-1552168800),
                end: NaiveTimestamp(-1552168800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1538344800),
                utc_start: NaiveTimestamp(-1538344800),
                end: NaiveTimestamp(-1538341200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1522530000),
                utc_start: NaiveTimestamp(-1522533600),
                end: NaiveTimestamp(-1522533600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1507500000),
                utc_start: NaiveTimestamp(-1507500000),
                end: NaiveTimestamp(-1507496400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1490562000),
                utc_start: NaiveTimestamp(-1490565600),
                end: NaiveTimestamp(-1490565600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1473631200),
                utc_start: NaiveTimestamp(-1473631200),
                end: NaiveTimestamp(-1473627600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1460926800),
                utc_start: NaiveTimestamp(-1460930400),
                end: NaiveTimestamp(-1460930400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1442786400),
                utc_start: NaiveTimestamp(-1442786400),
                end: NaiveTimestamp(-1442782800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1428872400),
                utc_start: NaiveTimestamp(-1428876000),
                end: NaiveTimestamp(-1428876000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1410732000),
                utc_start: NaiveTimestamp(-1410732000),
                end: NaiveTimestamp(-1410728400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1396213200),
                utc_start: NaiveTimestamp(-1396216800),
                end: NaiveTimestamp(-1396216800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1379282400),
                utc_start: NaiveTimestamp(-1379282400),
                end: NaiveTimestamp(-1379278800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1364763600),
                utc_start: NaiveTimestamp(-1364767200),
                end: NaiveTimestamp(-1364767200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1348437600),
                utc_start: NaiveTimestamp(-1348437600),
                end: NaiveTimestamp(-1348434000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1333314000),
                utc_start: NaiveTimestamp(-1333317600),
                end: NaiveTimestamp(-1333317600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1315778400),
                utc_start: NaiveTimestamp(-1315778400),
                end: NaiveTimestamp(-1315774800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1301259600),
                utc_start: NaiveTimestamp(-1301263200),
                end: NaiveTimestamp(-1301263200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1284328800),
                utc_start: NaiveTimestamp(-1284328800),
                end: NaiveTimestamp(-1284325200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1269810000),
                utc_start: NaiveTimestamp(-1269813600),
                end: NaiveTimestamp(-1269813600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1253484000),
                utc_start: NaiveTimestamp(-1253484000),
                end: NaiveTimestamp(-1253480400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1238360400),
                utc_start: NaiveTimestamp(-1238364000),
                end: NaiveTimestamp(-1238364000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1221429600),
                utc_start: NaiveTimestamp(-1221429600),
                end: NaiveTimestamp(-1221426000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1206910800),
                utc_start: NaiveTimestamp(-1206914400),
                end: NaiveTimestamp(-1206914400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1189980000),
                utc_start: NaiveTimestamp(-1189980000),
                end: NaiveTimestamp(-1189976400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1175461200),
                utc_start: NaiveTimestamp(-1175464800),
                end: NaiveTimestamp(-1175464800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1159135200),
                utc_start: NaiveTimestamp(-1159135200),
                end: NaiveTimestamp(-1159131600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1143406800),
                utc_start: NaiveTimestamp(-1143410400),
                end: NaiveTimestamp(-1143410400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1126476000),
                utc_start: NaiveTimestamp(-1126476000),
                end: NaiveTimestamp(-1126472400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1111957200),
                utc_start: NaiveTimestamp(-1111960800),
                end: NaiveTimestamp(-1111960800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1095631200),
                utc_start: NaiveTimestamp(-1095631200),
                end: NaiveTimestamp(-1095627600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1080507600),
                utc_start: NaiveTimestamp(-1080511200),
                end: NaiveTimestamp(-1080511200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1063576800),
                utc_start: NaiveTimestamp(-1063576800),
                end: NaiveTimestamp(-1063573200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1049058000),
                utc_start: NaiveTimestamp(-1049061600),
                end: NaiveTimestamp(-1049061600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1032127200),
                utc_start: NaiveTimestamp(-1032127200),
                end: NaiveTimestamp(-1032123600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-1017608400),
                utc_start: NaiveTimestamp(-1017612000),
                end: NaiveTimestamp(-1017612000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-1001282400),
                utc_start: NaiveTimestamp(-1001282400),
                end: NaiveTimestamp(-1001278800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-986158800),
                utc_start: NaiveTimestamp(-986162400),
                end: NaiveTimestamp(-986162400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-969228000),
                utc_start: NaiveTimestamp(-969228000),
                end: NaiveTimestamp(-969224400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-950475600),
                utc_start: NaiveTimestamp(-950479200),
                end: NaiveTimestamp(-950479200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-942012000),
                utc_start: NaiveTimestamp(-942012000),
                end: NaiveTimestamp(-942008400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 3,
                start: NaiveTimestamp(-904514400),
                utc_start: NaiveTimestamp(-904518000),
                end: NaiveTimestamp(-904510800),
                offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-896043600),
                utc_start: NaiveTimestamp(-896050800),
                end: NaiveTimestamp(-896047200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 3,
                start: NaiveTimestamp(-875484000),
                utc_start: NaiveTimestamp(-875487600),
                end: NaiveTimestamp(-875480400),
                offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-864594000),
                utc_start: NaiveTimestamp(-864601200),
                end: NaiveTimestamp(-864597600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 3,
                start: NaiveTimestamp(-844034400),
                utc_start: NaiveTimestamp(-844038000),
                end: NaiveTimestamp(-844030800),
                offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-832539600),
                utc_start: NaiveTimestamp(-832546800),
                end: NaiveTimestamp(-832543200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 3,
                start: NaiveTimestamp(-812584800),
                utc_start: NaiveTimestamp(-812588400),
                end: NaiveTimestamp(-812581200),
                offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-798066000),
                utc_start: NaiveTimestamp(-798073200),
                end: NaiveTimestamp(-798069600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 3,
                start: NaiveTimestamp(-781048800),
                utc_start: NaiveTimestamp(-781052400),
                end: NaiveTimestamp(-781045200),
                offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-772059600),
                utc_start: NaiveTimestamp(-772066800),
                end: NaiveTimestamp(-772063200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-764802000),
                utc_start: NaiveTimestamp(-764805600),
                end: NaiveTimestamp(-764805600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-748476000),
                utc_start: NaiveTimestamp(-748476000),
                end: NaiveTimestamp(-748472400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-733352400),
                utc_start: NaiveTimestamp(-733356000),
                end: NaiveTimestamp(-733356000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-719445600),
                utc_start: NaiveTimestamp(-719445600),
                end: NaiveTimestamp(-719442000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 3,
                start: NaiveTimestamp(-717026400),
                utc_start: NaiveTimestamp(-717030000),
                end: NaiveTimestamp(-717022800),
                offset: UtcOffset::from_hms(2, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-706741200),
                utc_start: NaiveTimestamp(-706748400),
                end: NaiveTimestamp(-706744800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-699483600),
                utc_start: NaiveTimestamp(-699487200),
                end: NaiveTimestamp(-699487200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-687996000),
                utc_start: NaiveTimestamp(-687996000),
                end: NaiveTimestamp(-687992400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-668034000),
                utc_start: NaiveTimestamp(-668037600),
                end: NaiveTimestamp(-668037600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-654732000),
                utc_start: NaiveTimestamp(-654732000),
                end: NaiveTimestamp(-654728400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-636584400),
                utc_start: NaiveTimestamp(-636588000),
                end: NaiveTimestamp(-636588000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-622072800),
                utc_start: NaiveTimestamp(-622072800),
                end: NaiveTimestamp(-622069200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-605739600),
                utc_start: NaiveTimestamp(-605743200),
                end: NaiveTimestamp(-605743200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-590623200),
                utc_start: NaiveTimestamp(-590623200),
                end: NaiveTimestamp(-590619600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-574290000),
                utc_start: NaiveTimestamp(-574293600),
                end: NaiveTimestamp(-574293600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-558568800),
                utc_start: NaiveTimestamp(-558568800),
                end: NaiveTimestamp(-558565200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-542235600),
                utc_start: NaiveTimestamp(-542239200),
                end: NaiveTimestamp(-542239200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-527119200),
                utc_start: NaiveTimestamp(-527119200),
                end: NaiveTimestamp(-527115600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-512600400),
                utc_start: NaiveTimestamp(-512604000),
                end: NaiveTimestamp(-512604000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-496274400),
                utc_start: NaiveTimestamp(-496274400),
                end: NaiveTimestamp(-496270800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-481150800),
                utc_start: NaiveTimestamp(-481154400),
                end: NaiveTimestamp(-481154400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-464220000),
                utc_start: NaiveTimestamp(-464220000),
                end: NaiveTimestamp(-464216400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-449701200),
                utc_start: NaiveTimestamp(-449704800),
                end: NaiveTimestamp(-449704800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-432165600),
                utc_start: NaiveTimestamp(-432165600),
                end: NaiveTimestamp(-432162000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-417646800),
                utc_start: NaiveTimestamp(-417650400),
                end: NaiveTimestamp(-417650400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-401320800),
                utc_start: NaiveTimestamp(-401320800),
                end: NaiveTimestamp(-401317200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-386197200),
                utc_start: NaiveTimestamp(-386200800),
                end: NaiveTimestamp(-386200800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-369266400),
                utc_start: NaiveTimestamp(-369266400),
                end: NaiveTimestamp(-369262800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-354747600),
                utc_start: NaiveTimestamp(-354751200),
                end: NaiveTimestamp(-354751200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-337816800),
                utc_start: NaiveTimestamp(-337816800),
                end: NaiveTimestamp(-337813200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-323298000),
                utc_start: NaiveTimestamp(-323301600),
                end: NaiveTimestamp(-323301600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-306972000),
                utc_start: NaiveTimestamp(-306972000),
                end: NaiveTimestamp(-306968400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-291848400),
                utc_start: NaiveTimestamp(-291852000),
                end: NaiveTimestamp(-291852000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-276732000),
                utc_start: NaiveTimestamp(-276732000),
                end: NaiveTimestamp(-276728400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-257979600),
                utc_start: NaiveTimestamp(-257983200),
                end: NaiveTimestamp(-257983200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-245282400),
                utc_start: NaiveTimestamp(-245282400),
                end: NaiveTimestamp(-245278800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-226530000),
                utc_start: NaiveTimestamp(-226533600),
                end: NaiveTimestamp(-226533600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-213228000),
                utc_start: NaiveTimestamp(-213228000),
                end: NaiveTimestamp(-213224400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-195080400),
                utc_start: NaiveTimestamp(-195084000),
                end: NaiveTimestamp(-195084000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-182383200),
                utc_start: NaiveTimestamp(-182383200),
                end: NaiveTimestamp(-182379600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-163630800),
                utc_start: NaiveTimestamp(-163634400),
                end: NaiveTimestamp(-163634400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-150933600),
                utc_start: NaiveTimestamp(-150933600),
                end: NaiveTimestamp(-150930000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-132181200),
                utc_start: NaiveTimestamp(-132184800),
                end: NaiveTimestamp(-132184800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-119484000),
                utc_start: NaiveTimestamp(-119484000),
                end: NaiveTimestamp(-119480400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-100731600),
                utc_start: NaiveTimestamp(-100735200),
                end: NaiveTimestamp(-100735200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-88034400),
                utc_start: NaiveTimestamp(-88034400),
                end: NaiveTimestamp(-88030800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(-68677200),
                utc_start: NaiveTimestamp(-68680800),
                end: NaiveTimestamp(-68680800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(-59004000),
                utc_start: NaiveTimestamp(-59004000),
                end: NaiveTimestamp(-59000400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 5,
                start: NaiveTimestamp(-37238400),
                utc_start: NaiveTimestamp(-37242000),
                end: NaiveTimestamp(-37238400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(57726000),
                utc_start: NaiveTimestamp(57722400),
                end: NaiveTimestamp(57722400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(69818400),
                utc_start: NaiveTimestamp(69818400),
                end: NaiveTimestamp(69822000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(89175600),
                utc_start: NaiveTimestamp(89172000),
                end: NaiveTimestamp(89172000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(101268000),
                utc_start: NaiveTimestamp(101268000),
                end: NaiveTimestamp(101271600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(120625200),
                utc_start: NaiveTimestamp(120621600),
                end: NaiveTimestamp(120621600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(132717600),
                utc_start: NaiveTimestamp(132717600),
                end: NaiveTimestamp(132721200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(152074800),
                utc_start: NaiveTimestamp(152071200),
                end: NaiveTimestamp(152071200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(164167200),
                utc_start: NaiveTimestamp(164167200),
                end: NaiveTimestamp(164170800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(183524400),
                utc_start: NaiveTimestamp(183520800),
                end: NaiveTimestamp(183520800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(196221600),
                utc_start: NaiveTimestamp(196221600),
                end: NaiveTimestamp(196225200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(214974000),
                utc_start: NaiveTimestamp(214970400),
                end: NaiveTimestamp(214970400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(227671200),
                utc_start: NaiveTimestamp(227671200),
                end: NaiveTimestamp(227674800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(246423600),
                utc_start: NaiveTimestamp(246420000),
                end: NaiveTimestamp(246420000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(259120800),
                utc_start: NaiveTimestamp(259120800),
                end: NaiveTimestamp(259124400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(278478000),
                utc_start: NaiveTimestamp(278474400),
                end: NaiveTimestamp(278474400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(290570400),
                utc_start: NaiveTimestamp(290570400),
                end: NaiveTimestamp(290574000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(309927600),
                utc_start: NaiveTimestamp(309924000),
                end: NaiveTimestamp(309924000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 1,
                start: NaiveTimestamp(322020000),
                utc_start: NaiveTimestamp(322020000),
                end: NaiveTimestamp(322023600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 2,
                start: NaiveTimestamp(341377200),
                utc_start: NaiveTimestamp(341373600),
                end: NaiveTimestamp(341373600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(354675600),
                utc_start: NaiveTimestamp(354675600),
                end: NaiveTimestamp(354679200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(372823200),
                utc_start: NaiveTimestamp(372819600),
                end: NaiveTimestamp(372819600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(386125200),
                utc_start: NaiveTimestamp(386125200),
                end: NaiveTimestamp(386128800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(404272800),
                utc_start: NaiveTimestamp(404269200),
                end: NaiveTimestamp(404269200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(417574800),
                utc_start: NaiveTimestamp(417574800),
                end: NaiveTimestamp(417578400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(435722400),
                utc_start: NaiveTimestamp(435718800),
                end: NaiveTimestamp(435718800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(449024400),
                utc_start: NaiveTimestamp(449024400),
                end: NaiveTimestamp(449028000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(467776800),
                utc_start: NaiveTimestamp(467773200),
                end: NaiveTimestamp(467773200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(481078800),
                utc_start: NaiveTimestamp(481078800),
                end: NaiveTimestamp(481082400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(499226400),
                utc_start: NaiveTimestamp(499222800),
                end: NaiveTimestamp(499222800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(512528400),
                utc_start: NaiveTimestamp(512528400),
                end: NaiveTimestamp(512532000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(530676000),
                utc_start: NaiveTimestamp(530672400),
                end: NaiveTimestamp(530672400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(543978000),
                utc_start: NaiveTimestamp(543978000),
                end: NaiveTimestamp(543981600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(562125600),
                utc_start: NaiveTimestamp(562122000),
                end: NaiveTimestamp(562122000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(575427600),
                utc_start: NaiveTimestamp(575427600),
                end: NaiveTimestamp(575431200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(593575200),
                utc_start: NaiveTimestamp(593571600),
                end: NaiveTimestamp(593571600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(606877200),
                utc_start: NaiveTimestamp(606877200),
                end: NaiveTimestamp(606880800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(625629600),
                utc_start: NaiveTimestamp(625626000),
                end: NaiveTimestamp(625626000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(638326800),
                utc_start: NaiveTimestamp(638326800),
                end: NaiveTimestamp(638330400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(657079200),
                utc_start: NaiveTimestamp(657075600),
                end: NaiveTimestamp(657075600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(670381200),
                utc_start: NaiveTimestamp(670381200),
                end: NaiveTimestamp(670384800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(688528800),
                utc_start: NaiveTimestamp(688525200),
                end: NaiveTimestamp(688525200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(701830800),
                utc_start: NaiveTimestamp(701830800),
                end: NaiveTimestamp(701834400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(719978400),
                utc_start: NaiveTimestamp(719974800),
                end: NaiveTimestamp(719974800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(733280400),
                utc_start: NaiveTimestamp(733280400),
                end: NaiveTimestamp(733284000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(751428000),
                utc_start: NaiveTimestamp(751424400),
                end: NaiveTimestamp(751424400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(764730000),
                utc_start: NaiveTimestamp(764730000),
                end: NaiveTimestamp(764733600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(782877600),
                utc_start: NaiveTimestamp(782874000),
                end: NaiveTimestamp(782874000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(796179600),
                utc_start: NaiveTimestamp(796179600),
                end: NaiveTimestamp(796183200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(814327200),
                utc_start: NaiveTimestamp(814323600),
                end: NaiveTimestamp(814323600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(828234000),
                utc_start: NaiveTimestamp(828234000),
                end: NaiveTimestamp(828237600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(846381600),
                utc_start: NaiveTimestamp(846378000),
                end: NaiveTimestamp(846378000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(859683600),
                utc_start: NaiveTimestamp(859683600),
                end: NaiveTimestamp(859687200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(877831200),
                utc_start: NaiveTimestamp(877827600),
                end: NaiveTimestamp(877827600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(891133200),
                utc_start: NaiveTimestamp(891133200),
                end: NaiveTimestamp(891136800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(909280800),
                utc_start: NaiveTimestamp(909277200),
                end: NaiveTimestamp(909277200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(922582800),
                utc_start: NaiveTimestamp(922582800),
                end: NaiveTimestamp(922586400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(941335200),
                utc_start: NaiveTimestamp(941331600),
                end: NaiveTimestamp(941331600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(954032400),
                utc_start: NaiveTimestamp(954032400),
                end: NaiveTimestamp(954036000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(972784800),
                utc_start: NaiveTimestamp(972781200),
                end: NaiveTimestamp(972781200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(985482000),
                utc_start: NaiveTimestamp(985482000),
                end: NaiveTimestamp(985485600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1004234400),
                utc_start: NaiveTimestamp(1004230800),
                end: NaiveTimestamp(1004230800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1017536400),
                utc_start: NaiveTimestamp(1017536400),
                end: NaiveTimestamp(1017540000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1035684000),
                utc_start: NaiveTimestamp(1035680400),
                end: NaiveTimestamp(1035680400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1048986000),
                utc_start: NaiveTimestamp(1048986000),
                end: NaiveTimestamp(1048989600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1067133600),
                utc_start: NaiveTimestamp(1067130000),
                end: NaiveTimestamp(1067130000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1080435600),
                utc_start: NaiveTimestamp(1080435600),
                end: NaiveTimestamp(1080439200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1099188000),
                utc_start: NaiveTimestamp(1099184400),
                end: NaiveTimestamp(1099184400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1111885200),
                utc_start: NaiveTimestamp(1111885200),
                end: NaiveTimestamp(1111888800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1130637600),
                utc_start: NaiveTimestamp(1130634000),
                end: NaiveTimestamp(1130634000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1143334800),
                utc_start: NaiveTimestamp(1143334800),
                end: NaiveTimestamp(1143338400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1162087200),
                utc_start: NaiveTimestamp(1162083600),
                end: NaiveTimestamp(1162083600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1174784400),
                utc_start: NaiveTimestamp(1174784400),
                end: NaiveTimestamp(1174788000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1193536800),
                utc_start: NaiveTimestamp(1193533200),
                end: NaiveTimestamp(1193533200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1206838800),
                utc_start: NaiveTimestamp(1206838800),
                end: NaiveTimestamp(1206842400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1224986400),
                utc_start: NaiveTimestamp(1224982800),
                end: NaiveTimestamp(1224982800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1238288400),
                utc_start: NaiveTimestamp(1238288400),
                end: NaiveTimestamp(1238292000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1256436000),
                utc_start: NaiveTimestamp(1256432400),
                end: NaiveTimestamp(1256432400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1269738000),
                utc_start: NaiveTimestamp(1269738000),
                end: NaiveTimestamp(1269741600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1288490400),
                utc_start: NaiveTimestamp(1288486800),
                end: NaiveTimestamp(1288486800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1301187600),
                utc_start: NaiveTimestamp(1301187600),
                end: NaiveTimestamp(1301191200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1319940000),
                utc_start: NaiveTimestamp(1319936400),
                end: NaiveTimestamp(1319936400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1332637200),
                utc_start: NaiveTimestamp(1332637200),
                end: NaiveTimestamp(1332640800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1351389600),
                utc_start: NaiveTimestamp(1351386000),
                end: NaiveTimestamp(1351386000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1364691600),
                utc_start: NaiveTimestamp(1364691600),
                end: NaiveTimestamp(1364695200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1382839200),
                utc_start: NaiveTimestamp(1382835600),
                end: NaiveTimestamp(1382835600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1396141200),
                utc_start: NaiveTimestamp(1396141200),
                end: NaiveTimestamp(1396144800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1414288800),
                utc_start: NaiveTimestamp(1414285200),
                end: NaiveTimestamp(1414285200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1427590800),
                utc_start: NaiveTimestamp(1427590800),
                end: NaiveTimestamp(1427594400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1445738400),
                utc_start: NaiveTimestamp(1445734800),
                end: NaiveTimestamp(1445734800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1459040400),
                utc_start: NaiveTimestamp(1459040400),
                end: NaiveTimestamp(1459044000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1477792800),
                utc_start: NaiveTimestamp(1477789200),
                end: NaiveTimestamp(1477789200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1490490000),
                utc_start: NaiveTimestamp(1490490000),
                end: NaiveTimestamp(1490493600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1509242400),
                utc_start: NaiveTimestamp(1509238800),
                end: NaiveTimestamp(1509238800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1521939600),
                utc_start: NaiveTimestamp(1521939600),
                end: NaiveTimestamp(1521943200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1540692000),
                utc_start: NaiveTimestamp(1540688400),
                end: NaiveTimestamp(1540688400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1553994000),
                utc_start: NaiveTimestamp(1553994000),
                end: NaiveTimestamp(1553997600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1572141600),
                utc_start: NaiveTimestamp(1572138000),
                end: NaiveTimestamp(1572138000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1585443600),
                utc_start: NaiveTimestamp(1585443600),
                end: NaiveTimestamp(1585447200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1603591200),
                utc_start: NaiveTimestamp(1603587600),
                end: NaiveTimestamp(1603587600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1616893200),
                utc_start: NaiveTimestamp(1616893200),
                end: NaiveTimestamp(1616896800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1635645600),
                utc_start: NaiveTimestamp(1635642000),
                end: NaiveTimestamp(1635642000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1648342800),
                utc_start: NaiveTimestamp(1648342800),
                end: NaiveTimestamp(1648346400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1667095200),
                utc_start: NaiveTimestamp(1667091600),
                end: NaiveTimestamp(1667091600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1679792400),
                utc_start: NaiveTimestamp(1679792400),
                end: NaiveTimestamp(1679796000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1698544800),
                utc_start: NaiveTimestamp(1698541200),
                end: NaiveTimestamp(1698541200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1711846800),
                utc_start: NaiveTimestamp(1711846800),
                end: NaiveTimestamp(1711850400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1729994400),
                utc_start: NaiveTimestamp(1729990800),
                end: NaiveTimestamp(1729990800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1743296400),
                utc_start: NaiveTimestamp(1743296400),
                end: NaiveTimestamp(1743300000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1761444000),
                utc_start: NaiveTimestamp(1761440400),
                end: NaiveTimestamp(1761440400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1774746000),
                utc_start: NaiveTimestamp(1774746000),
                end: NaiveTimestamp(1774749600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1792893600),
                utc_start: NaiveTimestamp(1792890000),
                end: NaiveTimestamp(1792890000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1806195600),
                utc_start: NaiveTimestamp(1806195600),
                end: NaiveTimestamp(1806199200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1824948000),
                utc_start: NaiveTimestamp(1824944400),
                end: NaiveTimestamp(1824944400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1837645200),
                utc_start: NaiveTimestamp(1837645200),
                end: NaiveTimestamp(1837648800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1856397600),
                utc_start: NaiveTimestamp(1856394000),
                end: NaiveTimestamp(1856394000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1869094800),
                utc_start: NaiveTimestamp(1869094800),
                end: NaiveTimestamp(1869098400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1887847200),
                utc_start: NaiveTimestamp(1887843600),
                end: NaiveTimestamp(1887843600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1901149200),
                utc_start: NaiveTimestamp(1901149200),
                end: NaiveTimestamp(1901152800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1919296800),
                utc_start: NaiveTimestamp(1919293200),
                end: NaiveTimestamp(1919293200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1932598800),
                utc_start: NaiveTimestamp(1932598800),
                end: NaiveTimestamp(1932602400),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1950746400),
                utc_start: NaiveTimestamp(1950742800),
                end: NaiveTimestamp(1950742800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1964048400),
                utc_start: NaiveTimestamp(1964048400),
                end: NaiveTimestamp(1964052000),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(1982800800),
                utc_start: NaiveTimestamp(1982797200),
                end: NaiveTimestamp(1982797200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(1995498000),
                utc_start: NaiveTimestamp(1995498000),
                end: NaiveTimestamp(1995501600),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(2014250400),
                utc_start: NaiveTimestamp(2014246800),
                end: NaiveTimestamp(2014246800),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(2026947600),
                utc_start: NaiveTimestamp(2026947600),
                end: NaiveTimestamp(2026951200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(2045700000),
                utc_start: NaiveTimestamp(2045696400),
                end: NaiveTimestamp(2045696400),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(2058397200),
                utc_start: NaiveTimestamp(2058397200),
                end: NaiveTimestamp(2058400800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(2077149600),
                utc_start: NaiveTimestamp(2077146000),
                end: NaiveTimestamp(2077146000),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(2090451600),
                utc_start: NaiveTimestamp(2090451600),
                end: NaiveTimestamp(2090455200),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(2108599200),
                utc_start: NaiveTimestamp(2108595600),
                end: NaiveTimestamp(2108595600),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 6,
                start: NaiveTimestamp(2121901200),
                utc_start: NaiveTimestamp(2121901200),
                end: NaiveTimestamp(2121904800),
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            },
            Transition {
                name_idx: 7,
                start: NaiveTimestamp(2140048800),
                utc_start: NaiveTimestamp(2140045200),
                end: NaiveTimestamp(2140045200),
                offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            },
        ],
        ttypes: &[
            TransitionType {
                offset: -75,
                is_dst: false,
                abbr: "LMT",
            },
            TransitionType {
                offset: 3600,
                is_dst: true,
                abbr: "BST",
            },
            TransitionType {
                offset: 0,
                is_dst: false,
                abbr: "GMT",
            },
            TransitionType {
                offset: 7200,
                is_dst: true,
                abbr: "BDST",
            },
            TransitionType {
                offset: 0,
                is_dst: false,
                abbr: "GMT",
            },
            TransitionType {
                offset: 3600,
                is_dst: false,
                abbr: "BST",
            },
            TransitionType {
                offset: 3600,
                is_dst: true,
                abbr: "BST",
            },
            TransitionType {
                offset: 0,
                is_dst: false,
                abbr: "GMT",
            },
        ],
        posix: &PosixTimeZone {
            std_abbr: "GMT",
            std_offset: UtcOffset::from_hms(0, 0, 0).unwrap(),
            dst: Some(&DstTransitionInfo {
                abbr: "BST",
                offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
                start: DstTransitionRule {
                    month: 3,
                    n: 5,
                    weekday: 0,
                    offset: 3600,
                },
                end: DstTransitionRule {
                    month: 10,
                    n: 5,
                    weekday: 0,
                    offset: 7200,
                },
                base_offset: UtcOffset::from_hms(1, 0, 0).unwrap(),
            }),
        },
    },
};

impl Transition {
    fn is_ambiguous(&self, ts: NaiveTimestamp) -> bool {
        self.end <= ts && ts < self.start
    }

    fn is_missing(&self, ts: NaiveTimestamp) -> bool {
        self.start <= ts && ts < self.end
    }
}
