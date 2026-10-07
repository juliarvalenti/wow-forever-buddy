//! Goal proposals (G1): a level for one character, or gold for one character
//! or the whole account, applied through `goals::add` like one set in the
//! app. The only free text is the player-facing gold label, held to the same
//! 24 characters as the app's field.

use serde::{Deserialize, Serialize};

use crate::characters;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::goals::{self, GoalKind, NewGoal};

/// The `propose_goal` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalProposal {
    /// Omitted for an account gold goal.
    #[serde(default)]
    pub character: Option<String>,
    pub kind: GoalKind,
    /// A level, or whole gold (not copper).
    pub target: u64,
    /// Gold only: "for the mount".
    #[serde(default)]
    pub label: Option<String>,
    /// YYYY-MM-DD.
    #[serde(default)]
    pub by: Option<String>,
}

/// After checking: what's stored and applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoalBody {
    goal: NewGoal,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct GoalView {
    /// `None` for an account gold goal.
    pub character_id: Option<u32>,
    pub character: Option<String>,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
    pub kind: GoalKind,
    /// A level, or copper, as stored.
    pub target: f64,
    pub label: Option<String>,
    /// RFC 3339.
    pub by: Option<String>,
}

fn err(e: AppError) -> String {
    match e {
        AppError::NotFound(m) | AppError::InvalidSettings(m) => m,
        e => e.to_string(),
    }
}

pub(crate) fn check(db: &Db, flavor: &str, p: &GoalProposal, now: i64) -> Result<GoalBody, String> {
    let character_id = match &p.character {
        Some(name) => Some(characters::by_name(db, flavor, name).map_err(err)?.id),
        None => None,
    };
    let by = match &p.by {
        None => None,
        Some(day) => Some(
            super::end_of_day(day).ok_or_else(|| format!("{day:?} isn't a date like 2026-10-09"))?
                as f64,
        ),
    };
    let target = match p.kind {
        GoalKind::Gold => p.target.saturating_mul(10_000),
        GoalKind::Level => p.target,
    };
    let goal = NewGoal {
        character_id,
        kind: p.kind,
        target: target as f64,
        label: p
            .label
            .as_ref()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty()),
        by,
    };
    goals::validate(db, flavor, &goal, now).map_err(err)?;
    Ok(GoalBody { goal })
}

pub(super) fn view(db: &Db, flavor: &str, b: &GoalBody) -> AppResult<GoalView> {
    let g = &b.goal;
    let card = match g.character_id {
        Some(id) => characters::overview(db, flavor)?
            .characters
            .into_iter()
            .find(|c| c.id == id),
        None => None,
    };
    Ok(GoalView {
        character_id: g.character_id,
        character: card.as_ref().map(characters::full_name),
        class: card.and_then(|c| c.class),
        kind: g.kind,
        target: g.target,
        label: g.label.clone(),
        by: g
            .by
            .and_then(|t| chrono::DateTime::from_timestamp(t as i64, 0))
            .map(|d| d.to_rfc3339()),
    })
}

/// Approve: the goal is added, checked again against today's data.
pub(super) fn apply(
    db: &Db,
    flavor: &str,
    b: &GoalBody,
    producer: &str,
    now: i64,
) -> AppResult<()> {
    goals::add(db, flavor, &b.goal, &format!("agent:{producer}"), now)?;
    Ok(())
}
