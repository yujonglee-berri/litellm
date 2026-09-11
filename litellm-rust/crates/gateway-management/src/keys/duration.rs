const WORD_ALIASES: &[(&str, &str)] = &[
    ("hourly", "1h"),
    ("daily", "24h"),
    ("weekly", "7d"),
    ("monthly", "30d"),
];

pub fn duration_in_seconds(duration: &str) -> Result<i64, String> {
    let normalized = WORD_ALIASES
        .iter()
        .find(|(word, _)| duration.eq_ignore_ascii_case(word))
        .map(|(_, alias)| *alias)
        .unwrap_or(duration);

    let split = normalized
        .char_indices()
        .find(|(_, character)| !character.is_ascii_digit())
        .map(|(index, _)| index)
        .unwrap_or(normalized.len());
    if split == 0 {
        return Err(format!("invalid duration: {duration}"));
    }
    let value: i64 = normalized[..split]
        .parse()
        .map_err(|_| format!("invalid duration: {duration}"))?;
    let seconds = match &normalized[split..] {
        "s" => value,
        "m" => value.saturating_mul(60),
        "h" => value.saturating_mul(3600),
        "d" => value.saturating_mul(86400),
        "w" => value.saturating_mul(604800),
        "mo" => value.saturating_mul(86400 * 30),
        other => {
            return Err(format!(
                "unsupported duration unit {other:?}, passed duration: {duration}"
            ));
        }
    };
    Ok(seconds)
}

#[cfg(test)]
mod tests {
    use super::duration_in_seconds;

    #[test]
    fn parses_python_duration_units() {
        assert_eq!(duration_in_seconds("30s").expect("seconds"), 30);
        assert_eq!(duration_in_seconds("30m").expect("minutes"), 1800);
        assert_eq!(duration_in_seconds("2h").expect("hours"), 7200);
        assert_eq!(duration_in_seconds("1d").expect("days"), 86400);
        assert_eq!(duration_in_seconds("1w").expect("weeks"), 604800);
        assert_eq!(duration_in_seconds("1mo").expect("months"), 2_592_000);
        assert_eq!(duration_in_seconds("hourly").expect("hourly"), 3600);
        assert_eq!(duration_in_seconds("daily").expect("daily"), 86400);
        assert_eq!(duration_in_seconds("DAILY").expect("case"), 86400);
    }

    #[test]
    fn rejects_unknown_units() {
        assert!(duration_in_seconds("30x").is_err());
        assert!(duration_in_seconds("abc").is_err());
        assert!(duration_in_seconds("").is_err());
    }
}
