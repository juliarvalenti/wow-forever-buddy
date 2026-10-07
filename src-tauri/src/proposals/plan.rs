//! Quest plan proposals (P2c, spec §3): a plan for one character, applied
//! through P1's `plans::set_plan`, which replaces any active plan. The
//! preview shows the active plan next to the new one (IMPLEMENTING §17).

use serde::{Deserialize, Serialize};

use crate::characters;
use crate::db::Db;
use crate::error::{AppError, AppResult};
use crate::plans::{self, Plan, Step, StepKind};

/// A step as an agent sends it: text, and optionally the quest and zone.
/// Where to go comes from the app's own quest log, never from the agent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProposedStep {
    pub text: String,
    #[serde(default)]
    pub quest_id: Option<u32>,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub kind: Option<StepKind>,
}

/// The `propose_quest_plan` tool's arguments, in the inbox file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanProposal {
    pub character: String,
    pub title: String,
    pub steps: Vec<ProposedStep>,
}

/// After checking: what's stored and applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct PlanBody {
    character_id: u32,
    title: String,
    steps: Vec<Step>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct PlanView {
    pub character_id: u32,
    pub character: String,
    /// File token, lowercase, for the class colour.
    pub class: Option<String>,
    pub title: String,
    pub steps: Vec<Step>,
    /// The character's active plan, which approving replaces.
    pub replaces: Option<Plan>,
}

fn plain(s: &str) -> String {
    s.split(|c: char| c.is_control())
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

pub(crate) fn check(db: &Db, flavor: &str, p: &PlanProposal) -> Result<PlanBody, String> {
    let c = characters::by_name(db, flavor, &p.character).map_err(|e| match e {
        AppError::NotFound(m) => m,
        e => e.to_string(),
    })?;
    let title = plain(&p.title);
    let steps: Vec<Step> = p
        .steps
        .iter()
        .map(|s| Step {
            text: plain(&s.text),
            quest_id: s.quest_id,
            zone: s.zone.as_deref().map(plain).filter(|z| !z.is_empty()),
            kind: s.kind,
        })
        .collect();
    plans::validate(&title, &steps).map_err(|e| match e {
        AppError::InvalidSettings(m) => m,
        e => e.to_string(),
    })?;
    Ok(PlanBody {
        character_id: c.id,
        title,
        steps,
    })
}

pub(super) fn view(db: &Db, flavor: &str, b: &PlanBody) -> AppResult<PlanView> {
    let card = characters::overview(db, flavor)?
        .characters
        .into_iter()
        .find(|c| c.id == b.character_id);
    let replaces = plans::active(db, flavor)?
        .into_iter()
        .find(|p| p.character_id == b.character_id);
    Ok(PlanView {
        character_id: b.character_id,
        character: card.as_ref().map(characters::full_name).unwrap_or_default(),
        class: card.and_then(|c| c.class),
        title: b.title.clone(),
        steps: b.steps.clone(),
        replaces,
    })
}

/// Approve: the stored plan becomes the character's active plan.
pub(super) fn apply(db: &Db, b: &PlanBody, producer: &str) -> AppResult<()> {
    plans::set_plan(
        db,
        b.character_id,
        &b.title,
        &b.steps,
        &format!("agent:{producer}"),
    )?;
    Ok(())
}
