//! The text files that say which build is installed: `.build.info` at the
//! WoW root, `.flavor.info` in the flavor folder, and the build config under
//! `Data/config`. <https://wowdev.wiki/TACT#Build_Config>

use super::{CascError, CascResult, Key};

/// The installed build for one product, from `.build.info`.
#[derive(Debug, Clone, PartialEq)]
pub struct BuildInfo {
    pub build_key: Key,
    /// e.g. `1.60.1.70205`.
    pub version: String,
}

/// The row for `product` in `.build.info` (a `Name!TYPE:size|...` header
/// line, then one `|`-separated row per installed product).
pub fn build_info(text: &str, product: &str) -> CascResult<BuildInfo> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<&str> = lines
        .next()
        .ok_or(CascError::Bad(".build.info"))?
        .split('|')
        .map(|h| h.split('!').next().unwrap_or(""))
        .collect();
    let col = |name: &str| header.iter().position(|h| h.eq_ignore_ascii_case(name));
    let (Some(key_col), Some(version_col), Some(product_col)) =
        (col("Build Key"), col("Version"), col("Product"))
    else {
        return Err(CascError::Bad(".build.info"));
    };
    for line in lines {
        let row: Vec<&str> = line.split('|').collect();
        if row.get(product_col).map(|p| p.trim()) != Some(product) {
            continue;
        }
        let build_key = row
            .get(key_col)
            .and_then(|k| hex_key(k))
            .ok_or(CascError::Bad(".build.info"))?;
        let version = row.get(version_col).unwrap_or(&"").trim().to_string();
        return Ok(BuildInfo { build_key, version });
    }
    Err(CascError::Missing(format!("{product} in .build.info")))
}

/// The product a flavor folder belongs to: the value row of `.flavor.info`
/// (`Product Flavor!STRING:0` then e.g. `wow_classic_beta`).
pub fn flavor_product(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .nth(1)
        .map(str::to_string)
}

/// The root content key and encoding encoding key from a build config
/// (`name = value ...` lines).
pub fn build_config(text: &str) -> CascResult<(Key, Key)> {
    let field = |name: &str| -> Option<Vec<&str>> {
        text.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == name).then(|| v.split_whitespace().collect())
        })
    };
    let root = field("root")
        .and_then(|v| hex_key(v.first()?))
        .ok_or(CascError::Bad("build config root"))?;
    // `encoding = <ckey> <ekey>`: the encoding file is looked up by its ekey.
    let encoding = field("encoding")
        .and_then(|v| hex_key(v.get(1)?))
        .ok_or(CascError::Bad("build config encoding"))?;
    Ok((root, encoding))
}

pub fn hex_key(s: &str) -> Option<Key> {
    let s = s.trim();
    if s.len() != 32 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut key = [0; 16];
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(key)
}

pub fn hex(key: &Key) -> String {
    key.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUILD_INFO: &str = "Branch!STRING:0|Active!DEC:1|Build Key!HEX:16|CDN Key!HEX:16|Version!STRING:0|Product!STRING:0\n\
        us|1|00112233445566778899aabbccddeeff|ffeeddccbbaa99887766554433221100|11.2.5.63000|wow\n\
        us|1|0123456789abcdef0123456789abcdef|ffeeddccbbaa99887766554433221100|1.60.1.70205|wow_classic_beta\n";

    #[test]
    fn picks_the_row_for_the_product() {
        let info = build_info(BUILD_INFO, "wow_classic_beta").unwrap();
        assert_eq!(hex(&info.build_key), "0123456789abcdef0123456789abcdef");
        assert_eq!(info.version, "1.60.1.70205");
        assert!(matches!(
            build_info(BUILD_INFO, "wow_classic_era"),
            Err(CascError::Missing(_))
        ));
        assert!(build_info("", "wow").is_err());
    }

    #[test]
    fn reads_the_flavor_product() {
        let text = "Product Flavor!STRING:0\nwow_classic_beta\n";
        assert_eq!(flavor_product(text).as_deref(), Some("wow_classic_beta"));
        assert_eq!(flavor_product("Product Flavor!STRING:0\n"), None);
    }

    #[test]
    fn reads_root_and_the_encoding_ekey() {
        let text = "# Build Configuration\n\n\
            root = 00000000000000000000000000000001\n\
            install = 00000000000000000000000000000002 00000000000000000000000000000003\n\
            encoding = 00000000000000000000000000000004 00000000000000000000000000000005\n\
            encoding-size = 1 2\n";
        let (root, encoding) = build_config(text).unwrap();
        assert_eq!(root[15], 1);
        assert_eq!(encoding[15], 5);
        assert!(build_config("root = zz").is_err());
    }
}
