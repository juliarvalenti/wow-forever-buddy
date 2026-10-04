//! Which snapshots to keep (spec §5 "Retention"). Pure functions over
//! snapshot metadata, so every rule is unit-tested without touching disk.
//!
//! - Manual and pinned: kept until the user deletes them, always.
//! - Automatic (app start, game exit, scheduled): everything from the last
//!   48 h, then the newest per day for 14 days, then the newest per week for
//!   8 weeks. Older ones go.
//! - Safety (pre-write, pre-restore): 30 days, and always at least the last 20.
//! - Over the storage budget: oldest automatic, then oldest safety
//!   snapshots go next, but never the newest 3 of each kind.
//!
//! Days and weeks are UTC calendar days and ISO weeks.

use std::collections::HashSet;

use chrono::{DateTime, Datelike, Duration, Utc};

use crate::backup::manifest::{SnapshotKind, SnapshotSummary};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Policy {
    pub keep_all_hours: i64,
    pub daily_days: i64,
    pub weekly_weeks: i64,
    pub safety_days: i64,
    pub safety_min: usize,
    /// Soft cap on the store's size on disk (after dedup and compression).
    pub budget_bytes: u64,
    /// Over budget, never go below this many newest snapshots per kind.
    pub budget_floor: usize,
}

pub const POLICY: Policy = Policy {
    keep_all_hours: 48,
    daily_days: 14,
    weekly_weeks: 8,
    safety_days: 30,
    safety_min: 20,
    budget_bytes: 5 * 1024 * 1024 * 1024,
    budget_floor: 3,
};

impl Policy {
    /// The sentence the Backups screen shows, generated from the numbers so
    /// the copy can't drift from the rules.
    pub fn summary(&self) -> String {
        format!(
            "Automatic: everything from the last {} h, then one a day for {} days and one a week for {} weeks. \
             Safety: {} days (at least the last {}). Manual and pinned: kept until you delete them.",
            self.keep_all_hours, self.daily_days, self.weekly_weeks, self.safety_days, self.safety_min
        )
    }
}

fn created(s: &SnapshotSummary) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s.created_at)
        .map(|d| d.with_timezone(&Utc))
        // An unparseable date sorts as brand new, which keeps it.
        .unwrap_or(DateTime::<Utc>::MAX_UTC)
}

/// Never pruned, by any rule.
pub fn is_protected(s: &SnapshotSummary) -> bool {
    s.pinned || s.kind == SnapshotKind::Manual
}

/// Snapshots the time rules say to delete (ids).
pub fn expired(snapshots: &[SnapshotSummary], now: DateTime<Utc>, p: &Policy) -> Vec<String> {
    let mut out = Vec::new();

    // Automatic: newest first, so the first one seen in a day/week is kept.
    let mut auto: Vec<&SnapshotSummary> = snapshots
        .iter()
        .filter(|s| s.kind == SnapshotKind::Auto && !is_protected(s))
        .collect();
    auto.sort_by_key(|s| std::cmp::Reverse(created(s)));
    let mut days_kept = HashSet::new();
    let mut weeks_kept = HashSet::new();
    for s in auto {
        let at = created(s);
        let age = now - at;
        let keep = if age <= Duration::hours(p.keep_all_hours) {
            true
        } else if age <= Duration::days(p.daily_days) {
            days_kept.insert(at.date_naive())
        } else if age <= Duration::weeks(p.weekly_weeks) {
            let week = at.iso_week();
            weeks_kept.insert((week.year(), week.week()))
        } else {
            false
        };
        if !keep {
            out.push(s.id.clone());
        }
    }

    // Safety: keep the newest `safety_min`, and anything younger than `safety_days`.
    let mut safety: Vec<&SnapshotSummary> = snapshots
        .iter()
        .filter(|s| s.kind == SnapshotKind::Safety && !is_protected(s))
        .collect();
    safety.sort_by_key(|s| std::cmp::Reverse(created(s)));
    for (i, s) in safety.into_iter().enumerate() {
        if i >= p.safety_min && now - created(s) > Duration::days(p.safety_days) {
            out.push(s.id.clone());
        }
    }
    out
}

/// When the store is over budget: the order to delete further snapshots in
/// (oldest automatic first, then oldest safety), never touching protected
/// ones or the newest `budget_floor` of each kind.
pub fn budget_candidates(snapshots: &[SnapshotSummary], p: &Policy) -> Vec<String> {
    let mut out = Vec::new();
    for kind in [SnapshotKind::Auto, SnapshotKind::Safety] {
        let mut of_kind: Vec<&SnapshotSummary> = snapshots
            .iter()
            .filter(|s| s.kind == kind && !is_protected(s))
            .collect();
        of_kind.sort_by_key(|s| created(s));
        let deletable = of_kind.len().saturating_sub(p.budget_floor);
        out.extend(of_kind.into_iter().take(deletable).map(|s| s.id.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backup::manifest::{Scope, Trigger};

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-04T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn snap(id: &str, trigger: Trigger, hours_ago: i64) -> SnapshotSummary {
        SnapshotSummary {
            id: id.into(),
            created_at: (now() - Duration::hours(hours_ago)).to_rfc3339(),
            trigger,
            kind: trigger.kind(),
            label: None,
            pinned: false,
            scope: Scope::Full,
            flavor: "_classic_beta_".into(),
            game_running: false,
            file_count: 1,
            char_count: 0,
            addon_count: 0,
            total_bytes: 0.0,
            new_bytes: 0.0,
        }
    }

    fn sorted(mut v: Vec<String>) -> Vec<String> {
        v.sort();
        v
    }

    #[test]
    fn keeps_everything_recent_then_daily_then_weekly() {
        let snaps = vec![
            snap("a-1h", Trigger::GameExit, 1),
            snap("a-30h", Trigger::GameExit, 30),
            snap("a-47h", Trigger::Scheduled, 47),
            // Day 3: two backups, only the newer survives.
            snap("d3-new", Trigger::GameExit, 72),
            snap("d3-old", Trigger::GameExit, 75),
            // Day 10: one backup, kept.
            snap("d10", Trigger::AppStart, 24 * 10),
            // Weeks 3 and 4 back: newest per ISO week.
            snap("w3-new", Trigger::GameExit, 24 * 20),
            snap("w3-old", Trigger::GameExit, 24 * 20 + 2),
            snap("w5", Trigger::GameExit, 24 * 35),
            // Past 8 weeks: gone.
            snap("ancient", Trigger::GameExit, 24 * 70),
        ];
        assert_eq!(
            sorted(expired(&snaps, now(), &POLICY)),
            ["ancient", "d3-old", "w3-old"]
        );
    }

    #[test]
    fn manual_and_pinned_are_never_expired() {
        let mut pinned_auto = snap("pinned", Trigger::GameExit, 24 * 400);
        pinned_auto.pinned = true;
        let snaps = vec![
            snap("manual", Trigger::Manual, 24 * 400),
            pinned_auto,
            snap("old-auto", Trigger::GameExit, 24 * 400),
        ];
        assert_eq!(expired(&snaps, now(), &POLICY), ["old-auto"]);
        assert_eq!(
            budget_candidates(&snaps, &POLICY),
            Vec::<String>::new(),
            "floor of 3"
        );
    }

    #[test]
    fn safety_keeps_thirty_days_and_at_least_twenty() {
        // 25 safety snapshots, all 40 days old: the newest 20 stay.
        let snaps: Vec<_> = (0..25)
            .map(|i| snap(&format!("s{i:02}"), Trigger::PreWrite, 24 * 40 + i))
            .collect();
        assert_eq!(
            sorted(expired(&snaps, now(), &POLICY)),
            ["s20", "s21", "s22", "s23", "s24"]
        );

        // Young ones are kept no matter how many there are.
        let young: Vec<_> = (0..30)
            .map(|i| snap(&format!("y{i}"), Trigger::PreRestore, i))
            .collect();
        assert!(expired(&young, now(), &POLICY).is_empty());
    }

    #[test]
    fn budget_takes_oldest_auto_then_safety_above_the_floor() {
        let mut snaps: Vec<_> = (0..5)
            .map(|i| snap(&format!("auto{i}"), Trigger::GameExit, i))
            .collect();
        snaps.extend((0..4).map(|i| snap(&format!("safe{i}"), Trigger::PreWrite, i)));
        snaps.push(snap("manual", Trigger::Manual, 1000));
        // auto4/auto3 are the oldest autos beyond the newest 3; then safe3.
        assert_eq!(
            budget_candidates(&snaps, &POLICY),
            ["auto4", "auto3", "safe3"]
        );
    }

    #[test]
    fn unparseable_dates_are_kept() {
        let mut odd = snap("odd", Trigger::GameExit, 0);
        odd.created_at = "not a date".into();
        assert!(expired(&[odd], now(), &POLICY).is_empty());
    }

    #[test]
    fn summary_reflects_the_policy() {
        let s = POLICY.summary();
        assert!(s.contains("48 h") && s.contains("14 days") && s.contains("8 weeks"));
        assert!(s.contains("30 days") && s.contains("20"));
        assert!(!s.contains('—'), "no em dashes in UI copy");
    }
}
