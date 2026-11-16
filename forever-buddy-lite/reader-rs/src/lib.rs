//! ForeverBuddy Lite reader: reads the addon's SavedVariables and Auctionator's price database, and
//! answers questions about your characters and the auction house. A Rust port of
//! `forever-buddy-lite/reader` (the Node scripts).
//!
//! Layers, from the files up:
//! - [`lua`], [`cbor`]: parsers for the two formats the game writes.
//! - [`json`]: where parsed tables become JSON, and the JS conventions the output keeps.
//! - [`sources`]: typed data from each file: characters, the gear scan, price ladders, Auctionator's
//!   daily prices, QuestieDB names and vendors, the hidden-items list.
//! - [`config`]: every market threshold, with its reason.
//! - [`model`]: small pure pricing functions: usual price, ladder gaps, ladder verdicts.
//! - [`strategies`]: one function per market rule, and `analyze` to run them all.
//! - [`report`]: the analysis as text.
//! - [`backtest`]: replays rules on past days against a control.
//! - [`tools`], [`mcp`]: the character commands, on the CLI and as an MCP server.

pub mod backtest;
pub mod cbor;
pub mod config;
pub mod format;
pub mod json;
pub mod lua;
pub mod mcp;
pub mod model;
pub mod report;
pub mod sources;
pub mod strategies;
pub mod tools;
