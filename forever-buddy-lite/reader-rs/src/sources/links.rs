//! Item links as the client writes them: `|cff0070dd|Hitem:5201::::|h[Emberstone Staff]|h|r`.

use serde::Serialize;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ParsedLink {
    pub id: Option<u32>,
    pub name: Option<String>,
    pub quality: Option<&'static str>,
}

const QUALITY_BY_INDEX: [&str; 8] = ["poor", "common", "uncommon", "rare", "epic", "legendary", "artifact", "heirloom"];

fn quality_by_color(hex: &str) -> Option<&'static str> {
    Some(match hex.to_ascii_lowercase().as_str() {
        "9d9d9d" => "poor",
        "ffffff" => "common",
        "1eff00" => "uncommon",
        "0070dd" => "rare",
        "a335ee" => "epic",
        "ff8000" => "legendary",
        _ => return None,
    })
}

/// Reads the item id, name and quality out of a link. Newer clients write the quality as
/// `|cnIQ<n>:` instead of a colour.
pub fn parse_link(link: Option<&str>) -> ParsedLink {
    let Some(link) = link else { return ParsedLink::default() };
    let id = after(link, "|Hitem:").and_then(|rest| {
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        digits.parse().ok()
    });
    let name = after(link, "|h[").and_then(|rest| rest.find("]|h").map(|end| &rest[..end])).filter(|n| !n.is_empty() && !n.contains(']'));
    let quality = quality_index(link).or_else(|| color(link).and_then(quality_by_color));
    ParsedLink { id, name: name.map(String::from), quality }
}

fn after<'a>(s: &'a str, marker: &str) -> Option<&'a str> {
    s.find(marker).map(|i| &s[i + marker.len()..])
}

/// `|cnIQ3:` -> "rare".
fn quality_index(link: &str) -> Option<&'static str> {
    let rest = after(link, "|cnIQ")?;
    let mut chars = rest.chars();
    let digit = chars.next()?.to_digit(10)?;
    (chars.next() == Some(':')).then(|| QUALITY_BY_INDEX.get(digit as usize).copied()).flatten()
}

/// `|cff0070dd` or `|c0070dd` -> "0070dd": the first colour code that has six hex digits.
fn color(link: &str) -> Option<&str> {
    let hex6 = |s: &str| s.len() >= 6 && s.as_bytes()[..6].iter().all(u8::is_ascii_hexdigit);
    let mut rest = link;
    while let Some(i) = rest.find("|c") {
        let code = &rest[i + 2..];
        if let Some(stripped) = code.strip_prefix("ff").filter(|s| hex6(s)) {
            return Some(&stripped[..6]);
        }
        if hex6(code) {
            return Some(&code[..6]);
        }
        rest = code;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn item_links() {
        let l = parse_link(Some("|cff0070dd|Hitem:5201::::|h[Emberstone Staff]|h|r"));
        assert_eq!(l, ParsedLink { id: Some(5201), name: Some("Emberstone Staff".into()), quality: Some("rare") });
    }

    #[test]
    fn newer_quality_codes_and_bare_links() {
        let l = parse_link(Some("|cnIQ2:|Hitem:7357::::::1205::20:::::::|h[Elder's Hat of the Eagle]|h|r"));
        assert_eq!(l.quality, Some("uncommon"));
        assert_eq!(l.id, Some(7357));
        let bare = parse_link(Some("item:4368::::::"));
        assert_eq!(bare, ParsedLink::default());
        assert_eq!(parse_link(Some("|Hitem:9999|h[Leather Cap]|h")).name.as_deref(), Some("Leather Cap"));
        assert_eq!(parse_link(None), ParsedLink::default());
        assert_eq!(parse_link(Some("|cff9D9D9D|Hitem:1|h[Junk]|h")).quality, Some("poor"));
        // The JS only knew a lowercase "ff" alpha prefix; an uppercase one reads as an unknown colour.
        assert_eq!(parse_link(Some("|cFF9D9D9D|Hitem:1|h[Junk]|h")).quality, None);
    }
}
