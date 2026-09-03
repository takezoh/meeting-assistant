//! Settings family repository (writer: interface host), including artifact roots.

use crate::repo::Store;
use crate::Result;
use ma_core_types::RootId;
use std::path::Path;

pub fn insert_root(store: &mut Store, root: RootId, absolute_path: &Path) -> Result<()> {
    store.conn().execute(
        "INSERT INTO roots (root_id, absolute_path, is_default) VALUES (?1, ?2, 1)",
        (
            root.to_string(),
            absolute_path.to_string_lossy().into_owned(),
        ),
    )?;
    Ok(())
}

/// Relocate a root: exactly one row changes and every stored reference stays valid.
pub fn relocate_root(store: &mut Store, root: RootId, new_path: &Path) -> Result<usize> {
    let changed = store.conn().execute(
        "UPDATE roots SET absolute_path = ?2 WHERE root_id = ?1",
        (root.to_string(), new_path.to_string_lossy().into_owned()),
    )?;
    Ok(changed)
}

pub fn set_setting(store: &mut Store, key: &str, value_json: &str) -> Result<()> {
    store.conn().execute("INSERT INTO settings (key, value_json) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json", (key, value_json))?;
    Ok(())
}
