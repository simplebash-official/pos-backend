use crate::domain::settings::{DesignOption, DesignOptionType};

fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Checks one value against the control that declared it.
pub(crate) fn validate_option_value(
    option: &DesignOption,
    value: &serde_json::Value,
) -> Result<(), String> {
    let label = &option.label;
    match option.r#type {
        DesignOptionType::Toggle if value.is_boolean() => Ok(()),
        DesignOptionType::Toggle => Err(format!("'{label}' must be on or off")),
        DesignOptionType::Select => {
            let picked = value.as_str().unwrap_or_default();
            let known = option
                .choices
                .as_ref()
                .is_some_and(|choices| choices.iter().any(|choice| choice.value == picked));
            if known {
                Ok(())
            } else {
                Err(format!("'{label}' must be one of the listed choices"))
            }
        }
        DesignOptionType::Color if value.as_str().is_some_and(is_hex_color) => Ok(()),
        DesignOptionType::Color => Err(format!("'{label}' must be a colour like #1A2B3C")),
        DesignOptionType::Text => {
            let max = option.max_length.unwrap_or(0) as usize;
            match value.as_str() {
                Some(text) if text.chars().count() <= max => Ok(()),
                Some(_) => Err(format!("'{label}' can be at most {max} characters")),
                None => Err(format!("'{label}' must be text")),
            }
        }
    }
}
