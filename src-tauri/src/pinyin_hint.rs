use pinyin::ToPinyin;

pub fn build_pinyin_hint(text: &str) -> String {
    let mut parts = Vec::new();

    for ch in text.chars() {
        if ch.is_whitespace() {
            parts.push("/".to_string());
            continue;
        }

        if let Some(py) = ch.to_pinyin() {
            parts.push(py.plain().to_string());
        } else {
            parts.push(ch.to_string());
        }
    }

    parts.join(" ")
}
