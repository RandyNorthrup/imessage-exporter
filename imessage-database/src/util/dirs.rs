/*!
 Default local paths for Messages data.
*/

use std::{env::var, path::PathBuf};

use crate::tables::table::DEFAULT_PATH_MACOS;

/// Return the current user's home directory.
///
/// # Example:
///
/// ```
/// use imessage_database::util::dirs::home;
///
/// let path = home();
/// println!("{path}");
/// ```
#[must_use]
pub fn home() -> String {
    var("HOME").unwrap_or_default()
}

/// Return the default macOS Messages database path.
///
/// # Example:
///
/// ```
/// use imessage_database::util::dirs::default_db_path;
///
/// let path = default_db_path();
/// println!("{path:?}");
/// ```
#[must_use]
pub fn default_db_path() -> PathBuf {
    PathBuf::from(format!("{}/{DEFAULT_PATH_MACOS}", home()))
}
