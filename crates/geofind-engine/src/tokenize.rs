//! Lightweight query and name tokenization using only `std`.

/// Lowercases and splits text into alphanumeric tokens.
pub fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            current.extend(ch.to_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Joins tokens with spaces for index documents and queries.
pub fn normalize_query(input: &str) -> String {
    tokenize(input).join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_lowercases() {
        assert_eq!(
            tokenize("Pilkington Avenue, Birmingham"),
            vec!["pilkington", "avenue", "birmingham"]
        );
    }

    #[test]
    fn normalize_joins_tokens() {
        assert_eq!(normalize_query("  Foo-Bar  Baz "), "foo bar baz");
    }
}
