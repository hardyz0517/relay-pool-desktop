use crate::persistence::{error::PersistenceError, write_session::WriteSession};

const LEGACY_TRAY_BEHAVIOR_ALIASES: [(&str, &str); 2] = [
    ("minimize-to-tray", "minimize_to_tray"),
    ("close-to-tray", "close_to_tray"),
];

pub(crate) fn canonical_tray_behavior(value: &str) -> Option<&str> {
    match value {
        "minimize_to_tray" | "close_to_tray" | "disabled" => Some(value),
        legacy => LEGACY_TRAY_BEHAVIOR_ALIASES
            .iter()
            .find_map(|(alias, canonical)| (*alias == legacy).then_some(*canonical)),
    }
}

pub(crate) async fn repair_legacy_settings(
    write: &mut WriteSession,
) -> Result<u64, PersistenceError> {
    let mut repaired = 0;
    for (legacy, canonical) in LEGACY_TRAY_BEHAVIOR_ALIASES {
        repaired += sqlx::query(
            r#"
            UPDATE settings
            SET value = ?1
            WHERE key = 'tray_behavior' AND value = ?2
            "#,
        )
        .bind(canonical)
        .bind(legacy)
        .execute(write.connection())
        .await?
        .rows_affected();
    }
    Ok(repaired)
}

#[cfg(test)]
mod tests {
    use super::canonical_tray_behavior;

    #[test]
    fn tray_behavior_aliases_map_to_the_canonical_contract() {
        assert_eq!(
            canonical_tray_behavior("minimize-to-tray"),
            Some("minimize_to_tray")
        );
        assert_eq!(
            canonical_tray_behavior("close-to-tray"),
            Some("close_to_tray")
        );
        assert_eq!(canonical_tray_behavior("disabled"), Some("disabled"));
        assert_eq!(canonical_tray_behavior("unknown"), None);
    }
}
