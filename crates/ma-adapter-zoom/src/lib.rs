//! Data-only adapter for one service: the table in `adapter.toml` plus the shared match function.
//! Every service identifier this product knows about lives in a table like this one (layer L4).

use ma_signal::adapter::TableAdapter;

pub const TABLE: &str = include_str!("../adapter.toml");

/// The adapter instance the composition root registers.
pub fn adapter() -> TableAdapter {
    TableAdapter::from_toml(TABLE).expect("adapter table parses")
}
