//! Character folders in `WTF/Account/<account>/<group>/<character>`.
//!
//! Forever added surnames, and with them a new layout: the group is an opaque
//! id (`70`) and the character folder is the full name (`70/Ellygie-Vargur`,
//! or `70/Brannic` without a surname). An account can still have folders in
//! the layout from before, under a realm name (`Classic Beta PvP 2/Ellygie`,
//! probe run 1). Those are "older settings folders": listed apart, not
//! counted as characters, and backed up and restorable like any other folder.
//! It's decided by the layout, never by matching names, so it makes no claim
//! about which character an older folder belonged to.

use std::collections::HashSet;

/// Forever's group folders are opaque ids; so far all digits (`70`). If a
/// later probe shows other ids, this is the one place to change.
fn is_group_id(folder: &str) -> bool {
    !folder.is_empty() && folder.bytes().all(|b| b.is_ascii_digit())
}

/// Which of an account's group folders hold older settings folders: the
/// ones that aren't a group id, but only on an account that has the new
/// layout at all. An install that only has realm folders (Classic Era, or
/// Forever from before surnames) keeps every folder as a character.
pub fn older_groups<'a>(groups: impl IntoIterator<Item = &'a str>) -> HashSet<&'a str> {
    let groups: Vec<&str> = groups.into_iter().collect();
    if !groups.iter().any(|g| is_group_id(g)) {
        return HashSet::new();
    }
    groups.into_iter().filter(|g| !is_group_id(g)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reviewer's three fixtures: `70/Brannic` and `70/Ellygie-Vargur`
    /// stay characters, `Classic Beta PvP 2/Ellygie` is an older folder.
    #[test]
    fn realm_folders_next_to_a_group_id_are_older() {
        assert_eq!(
            older_groups(["70", "Classic Beta PvP 2"]),
            HashSet::from(["Classic Beta PvP 2"])
        );
    }

    #[test]
    fn realm_folders_alone_are_current() {
        assert!(older_groups(["Whitemane", "Classic Beta PvP 2"]).is_empty());
        assert!(older_groups(["70", "71"]).is_empty());
        assert!(older_groups([]).is_empty());
    }
}
