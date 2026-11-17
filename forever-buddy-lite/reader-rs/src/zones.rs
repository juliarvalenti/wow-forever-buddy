//! Zone names by area id, as QuestieDB and the game use them.

/// Zone names by area id (the classic world's zones and capitals).
pub fn zone_name(zone: u32) -> Option<&'static str> {
    Some(match zone {
        33 => "Stranglethorn",
        45 => "Arathi Highlands",
        36 => "Alterac Mountains",
        267 => "Hillsbrad",
        11 => "Wetlands",
        405 => "Desolace",
        15 => "Dustwallow Marsh",
        400 => "Thousand Needles",
        3 => "Badlands",
        8 => "Swamp of Sorrows",
        47 => "Hinterlands",
        440 => "Tanaris",
        357 => "Feralas",
        331 => "Ashenvale",
        406 => "Stonetalon",
        17 => "Barrens",
        1519 => "Stormwind",
        1537 => "Ironforge",
        10 => "Duskwood",
        44 => "Redridge",
        38 => "Loch Modan",
        4 => "Blasted Lands",
        16 => "Azshara",
        361 => "Felwood",
        51 => "Searing Gorge",
        46 => "Burning Steppes",
        1657 => "Darnassus",
        148 => "Darkshore",
        130 => "Silverpine",
        85 => "Tirisfal",
        28 => "Western Plaguelands",
        139 => "Eastern Plaguelands",
        41 => "Deadwind Pass",
        490 => "Un'Goro",
        12 => "Elwynn",
        40 => "Westfall",
        1 => "Dun Morogh",
        141 => "Teldrassil",
        14 => "Durotar",
        215 => "Mulgore",
        618 => "Winterspring",
        493 => "Moonglade",
        1377 => "Silithus",
        1638 => "Thunder Bluff",
        1637 => "Orgrimmar",
        1497 => "Undercity",
        _ => return None,
    })
}

pub fn dungeon_name(zone: u32) -> Option<&'static str> {
    Some(match zone {
        796 => "Scarlet Monastery",
        491 => "Razorfen Kraul",
        722 => "Razorfen Downs",
        1337 => "Uldaman",
        721 => "Gnomeregan",
        1176 => "Zul'Farrak",
        2100 => "Maraudon",
        209 => "Shadowfang Keep",
        718 => "Wailing Caverns",
        1581 => "Deadmines",
        717 => "Stockade",
        719 => "Blackfathom Deeps",
        2557 => "Dire Maul",
        1584 => "Blackrock Depths",
        _ => return None,
    })
}

/// A zone or dungeon's name: "zone 123" for one not listed, "an unknown zone" for none.
pub fn describe(zone: Option<u32>) -> String {
    match zone {
        Some(z) => zone_name(z).or_else(|| dungeon_name(z)).map_or_else(|| format!("zone {z}"), String::from),
        None => "an unknown zone".into(),
    }
}
