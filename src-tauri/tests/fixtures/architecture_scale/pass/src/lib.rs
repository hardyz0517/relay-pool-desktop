mod nested;
mod ordinary;
mod same_a;
mod same_b;

use crate::nested::{self as nested_alias, child::*};
pub use crate::ordinary::PublicValue as RenamedValue;
use crate::same_a::Same;
use crate::same_b::Same as OtherSame;

#[cfg(not(windows))]
mod must_not_be_resolved;

#[cfg(windows)]
mod windows_enabled;

#[cfg_attr(target_os = "windows", path = "windows_only.rs")]
mod platform;

mod inline {
    use super::ordinary::PublicValue;
    pub(crate) fn value() -> PublicValue { PublicValue }
}

fn exercise() {
    let _ = nested_alias::child::value();
    let _ = value();
    let _ = inline::value();
    let _ = (Same, OtherSame);
}

command_registry_fixture!("read_status", "write_status");
