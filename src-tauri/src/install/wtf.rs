//! Character folder names in `WTF/Account/<account>/<group>/<character>`.
//!
//! Forever added surnames, and with them a new layout: `70/Ellygie-Vargur`.
//! An account can still have the folder from before, `Classic Beta PvP 2/
//! Ellygie` (probe run 1). That's an older settings folder: it's shown muted,
//! labelled, and not counted as a character, but it stays a real folder that
//! is backed up and restorable like any other. Nothing is merged.

use std::collections::HashSet;

/// Which of an account's character folders are older pre-surname folders: a
/// bare name (no '-') that's the first name of a surname folder on the same
/// account, compared case-insensitively. Returns the folder names as given.
pub fn older_folders<'a>(names: impl IntoIterator<Item = &'a str>) -> HashSet<&'a str> {
    let names: Vec<&str> = names.into_iter().collect();
    let first_names: HashSet<String> = names
        .iter()
        .filter_map(|n| n.split_once('-'))
        .map(|(first, _)| first.to_lowercase())
        .collect();
    names
        .into_iter()
        .filter(|n| !n.contains('-') && first_names.contains(&n.to_lowercase()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bare_name_matching_a_surname_folder_is_older() {
        let older = older_folders([
            "Ellygie-Vargur",
            "ellygie", // the legacy folder, any case
            "Ellyanna-Vargur",
            "Brannic", // no surname folder: a character in its own right
            "Vargur",  // a surname, not a first name
        ]);
        assert_eq!(older, HashSet::from(["ellygie"]));
    }

    #[test]
    fn nothing_is_older_without_surname_folders() {
        assert!(older_folders(["Ellygie", "Brannic"]).is_empty());
    }
}
