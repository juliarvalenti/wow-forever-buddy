//! Talent trees: classic tabs as saved, or the newer traits UI split back into its three trees.

use crate::json::number_string;
use crate::sources::addon::{Character, TalentNode};
use serde::Serialize;
use serde_json::Value;

/// Tree names by class, in the order the game lays them out left to right.
fn tree_names(class: &str) -> Option<[&'static str; 3]> {
    Some(match class {
        "WARRIOR" => ["Arms", "Fury", "Protection"],
        "PALADIN" => ["Holy", "Protection", "Retribution"],
        "HUNTER" => ["Beast Mastery", "Marksmanship", "Survival"],
        "ROGUE" => ["Assassination", "Combat", "Subtlety"],
        "PRIEST" => ["Discipline", "Holy", "Shadow"],
        "SHAMAN" => ["Elemental", "Enhancement", "Restoration"],
        "MAGE" => ["Arcane", "Fire", "Frost"],
        "WARLOCK" => ["Affliction", "Demonology", "Destruction"],
        "DRUID" => ["Balance", "Feral Combat", "Restoration"],
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TalentSummary {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unspent: Option<Value>,
    pub trees: Vec<Tree>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tree {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<f64>,
    /// "Arcane Focus 5/5", in tree order.
    pub talents: Vec<String>,
}

/// A number as shown in text; "?" when missing.
fn show(n: Option<f64>) -> String {
    n.map_or_else(|| "?".into(), number_string)
}

fn talent_label(name: &str, node: &TalentNode) -> String {
    format!("{name} {}/{}", show(node.rank), show(node.max))
}

/// A node's name, or its spell id when the name wasn't saved.
fn node_name(node: &TalentNode) -> String {
    match (&node.name, &node.spell) {
        (Some(n), _) if !n.is_empty() => n.clone(),
        (_, Some(Value::String(s))) => format!("spell {s}"),
        (_, Some(v)) => format!("spell {}", v.as_f64().map_or_else(|| v.to_string(), number_string)),
        _ => "spell ?".into(),
    }
}

fn taken(node: &TalentNode) -> bool {
    node.rank.is_some_and(|r| r > 0.0)
}

pub fn talent_trees(c: &Character) -> Option<TalentSummary> {
    let t = c.talents.as_ref()?;
    if t.system.as_deref() != Some("traits") {
        let trees = t
            .tabs
            .iter()
            .map(|tab| Tree {
                name: tab.name.clone(),
                points: tab.points,
                talents: tab.talents.iter().filter(|x| taken(x)).map(|x| talent_label(x.name.as_deref().unwrap_or("?"), x)).collect(),
            })
            .collect();
        return Some(TalentSummary { system: t.system.clone(), unspent: t.unspent.clone(), trees, legacy: None });
    }
    let legacy: Vec<String> = t.legacy.iter().filter(|n| taken(n)).map(|n| talent_label(&node_name(n), n)).collect();
    Some(TalentSummary {
        system: Some("traits".into()),
        unspent: t.unspent.clone(),
        trees: split_traits(&t.nodes, c.identity.class.as_deref().unwrap_or("")),
        legacy: (!legacy.is_empty()).then_some(legacy),
    })
}

/// The traits UI puts all three trees side by side on one canvas. Columns within a tree are evenly
/// spaced; between trees the gap is much wider. So: split the columns wherever the gap is more than
/// 1.5x the usual (median) gap, as Leatrix Plus does.
fn split_traits(nodes: &[TalentNode], class: &str) -> Vec<Tree> {
    let mut columns: Vec<f64> = nodes.iter().filter_map(|n| n.x).filter(|x| x.is_finite()).collect();
    columns.sort_by(|a, b| a.partial_cmp(b).unwrap());
    columns.dedup();
    let mut gaps: Vec<f64> = columns.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let usual = if gaps.is_empty() { 0.0 } else { gaps[gaps.len().div_ceil(2) - 1] };
    let mut tree_of_column = Vec::with_capacity(columns.len());
    let mut tree = 0;
    for (k, x) in columns.iter().enumerate() {
        if k > 0 && x - columns[k - 1] > usual * 1.5 {
            tree += 1;
        }
        tree_of_column.push(tree);
    }
    let tree_count = tree_of_column.last().map_or(0, |t| t + 1);
    let names = tree_names(class);
    // Per tree: points spent, and the talents taken.
    let mut trees: Vec<(f64, Vec<&TalentNode>)> = vec![(0.0, Vec::new()); tree_count];
    for node in nodes {
        let Some(column) = node.x.and_then(|x| columns.iter().position(|c| *c == x)) else { continue };
        let (points, taken_nodes) = &mut trees[tree_of_column[column]];
        *points += node.rank.unwrap_or(0.0);
        if taken(node) {
            taken_nodes.push(node);
        }
    }
    trees
        .into_iter()
        .enumerate()
        .map(|(k, (points, mut taken_nodes))| {
            // Top to bottom, then left to right.
            let pos = |n: &TalentNode| (n.y.unwrap_or(0.0), n.x.unwrap_or(0.0));
            taken_nodes.sort_by(|a, b| pos(a).partial_cmp(&pos(b)).unwrap_or(std::cmp::Ordering::Equal));
            Tree {
                name: Some(names.and_then(|n| n.get(k).map(|s| s.to_string())).unwrap_or_else(|| format!("Tree {}", k + 1))),
                points: Some(points),
                talents: taken_nodes.iter().map(|n| talent_label(&node_name(n), n)).collect(),
            }
        })
        .collect()
}
