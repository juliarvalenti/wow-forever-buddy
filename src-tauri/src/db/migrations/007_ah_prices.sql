-- F5: auction prices from Auctionator's account-wide SavedVariables.
-- Auctionator prunes its own history after 21 days; these tables keep every
-- day they've seen, so history grows with play. One market per flavor and
-- Auctionator realm root ("Forever").

-- One row per item and day: Auctionator's lowest and highest minimum price
-- that day, and the most listed.
CREATE TABLE ah_prices (
    flavor    TEXT NOT NULL,
    realm     TEXT NOT NULL,
    item_key  TEXT NOT NULL,               -- "2589", "g:19019:180", "p:39"
    item_id   INTEGER,                     -- NULL for battle pets
    day       TEXT NOT NULL,               -- YYYY-MM-DD
    low       INTEGER NOT NULL,            -- copper
    high      INTEGER NOT NULL,
    available INTEGER,
    PRIMARY KEY (flavor, realm, item_key, day)
) STRICT;
CREATE INDEX ah_prices_item ON ah_prices (flavor, item_id, day);

-- The last minimum price Auctionator saw for each item, and on which day.
CREATE TABLE ah_latest (
    flavor   TEXT NOT NULL,
    realm    TEXT NOT NULL,
    item_key TEXT NOT NULL,
    item_id  INTEGER,
    price    INTEGER NOT NULL,
    day      TEXT NOT NULL,
    PRIMARY KEY (flavor, realm, item_key)
) STRICT;
CREATE INDEX ah_latest_item ON ah_latest (flavor, item_id);

-- When Auctionator last scanned, per account file (Unix seconds).
CREATE TABLE ah_scans (
    flavor       TEXT NOT NULL,
    account      TEXT NOT NULL,
    replicate_at INTEGER,                  -- full scan
    browse_at    INTEGER,                  -- incremental scan
    read_at      INTEGER NOT NULL,         -- when the app read the file
    PRIMARY KEY (flavor, account)
) STRICT;

-- The user's watchlist (the app's, never written to the game).
CREATE TABLE ah_watch (
    flavor   TEXT NOT NULL,
    item_id  INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (flavor, item_id)
) STRICT;
