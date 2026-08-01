pub(crate) fn mapped_model(model: Option<&str>, aliases: &[(String, String)]) -> Option<String> {
    let model = model?;
    aliases
        .iter()
        .find_map(|(client_model, upstream_model)| {
            (client_model == model).then(|| upstream_model.clone())
        })
        .or_else(|| Some(model.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mapped_model_prefers_configured_alias() {
        let aliases = vec![("client-model".to_string(), "upstream-model".to_string())];

        assert_eq!(
            mapped_model(Some("client-model"), &aliases).as_deref(),
            Some("upstream-model")
        );
    }

    #[test]
    fn mapped_model_falls_back_to_requested_model() {
        let aliases = vec![("client-model".to_string(), "upstream-model".to_string())];

        assert_eq!(
            mapped_model(Some("other-model"), &aliases).as_deref(),
            Some("other-model")
        );
    }
}
