//! ForeverBuddy Lite reader: reads the addon's SavedVariables and Auctionator's price database, and
//! answers questions about your characters and the auction house.
//!
//! Layers, from the files up:
//! - [`lua`], [`cbor`]: parsers for the two formats the game writes.
//! - [`json`]: where parsed tables become JSON.
//! - [`sources`]: typed data from each file: characters, the gear scan, price ladders, Auctionator's
//!   daily prices, QuestieDB, the hidden-items list.
//! - [`config`]: every market threshold, with its reason.
//! - [`model`]: small pure pricing functions: usual price, ladder gaps, ladder verdicts.
//! - [`strategies`]: one function per market rule, and `analyze` to run them all.
//! - [`report`]: the analysis as text; [`dashboard`] and [`catalog`]: as the web dashboard reads it.
//! - [`live`]: the watcher and web server behind the dashboard ([`history`]: its scans, item by item); [`wealth`]: gold and item value over time.
//! - [`backtest`]: replays rules on past days against a control.
//! - [`snipes`], [`skillup`], [`questxp`], [`bis`]: planning tools.
//! - [`tools`], [`mcp`]: the character commands, on the CLI and as an MCP server.

pub mod backtest;
pub mod bgs;
pub mod bis;
pub mod catalog;
pub mod cbor;
pub mod config;
pub mod dashboard;
pub mod dips;
pub mod format;
pub mod history;
pub mod journal;
pub mod json;
pub mod live;
pub mod lua;
pub mod mcp;
pub mod model;
pub mod npcs;
pub mod questxp;
pub mod report;
pub mod skillup;
pub mod snipes;
pub mod sources;
pub mod spawns;
pub mod strategies;
pub mod thinning;
pub mod tools;
pub mod wealth;
pub mod zones;
