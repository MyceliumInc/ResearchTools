const HTML_ENTITIES: &[(&str, &str)] = &[
    ("&nbsp;", " "),
    ("&amp;", "&"),
    ("&quot;", "\""),
    ("&#39;", "'"),
    ("&lt;", "<"),
    ("&gt;", ">"),
];

pub fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

pub fn strip_tags(input: &str) -> String {
    collapse_whitespace(&decode_entities(&remove_tags(input)))
}

pub fn strip_html_doc(input: &str) -> String {
    let without_scripts = remove_block(input, "script");
    let without_styles = remove_block(&without_scripts, "style");
    strip_tags(&without_styles)
}

pub fn strip_cdata(value: &str) -> &str {
    value
        .strip_prefix("<![CDATA[")
        .and_then(|inner| inner.strip_suffix("]]>"))
        .unwrap_or(value)
}

pub fn extract_xml_tag<'a>(text: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&close)? + start;
    Some(text[start..end].trim())
}

pub fn has_class(tag: &str, name: &str) -> bool {
    tag.contains(&format!("class=\"{}\"", name)) || tag.contains(&format!("class='{}'", name))
}

pub fn extract_attr(tag: &str, name: &str) -> String {
    for delimiter in ['"', '\''] {
        let needle = format!("{}={}", name, delimiter);
        if let Some(start) = tag.find(&needle) {
            let after = &tag[start + needle.len()..];
            if let Some(end) = after.find(delimiter) {
                return after[..end].to_string();
            }
        }
    }
    String::new()
}

fn remove_tags(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut in_tag = false;
    for character in input.chars() {
        match character {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => output.push(character),
            _ => {}
        }
    }
    output
}

fn decode_entities(input: &str) -> String {
    if !input.contains('&') {
        return input.to_string();
    }
    let mut output = String::with_capacity(input.len());
    let mut rest = input;
    while let Some(offset) = rest.find('&') {
        output.push_str(&rest[..offset]);
        let tail = &rest[offset..];
        match HTML_ENTITIES
            .iter()
            .find(|(entity, _)| tail.starts_with(entity))
        {
            Some((entity, replacement)) => {
                output.push_str(replacement);
                rest = &tail[entity.len()..];
            }
            None => {
                output.push('&');
                rest = &tail[1..];
            }
        }
    }
    output.push_str(rest);
    output
}

fn collapse_whitespace(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut at_boundary = true;
    for character in input.chars() {
        if character.is_whitespace() {
            if !at_boundary {
                output.push(' ');
                at_boundary = true;
            }
        } else {
            output.push(character);
            at_boundary = false;
        }
    }
    if output.ends_with(' ') {
        output.pop();
    }
    output
}

fn remove_block(input: &str, tag: &str) -> String {
    let open = format!("<{}", tag);
    let close = format!("</{}>", tag);
    let mut output = String::with_capacity(input.len());
    let mut cursor = input;
    loop {
        match find_ignore_case(cursor, &open) {
            Some(start) => {
                output.push_str(&cursor[..start]);
                let after_open = &cursor[start..];
                match find_ignore_case(after_open, &close) {
                    Some(end) => cursor = &after_open[end + close.len()..],
                    None => return output,
                }
            }
            None => {
                output.push_str(cursor);
                return output;
            }
        }
    }
}

fn find_ignore_case(haystack: &str, needle_lower: &str) -> Option<usize> {
    let hay = haystack.as_bytes();
    let needle = needle_lower.as_bytes();
    if needle.is_empty() || hay.len() < needle.len() {
        return None;
    }
    for start in 0..=hay.len() - needle.len() {
        let matches = hay[start..start + needle.len()]
            .iter()
            .zip(needle)
            .all(|(byte, target)| byte.to_ascii_lowercase() == *target);
        if matches {
            return Some(start);
        }
    }
    None
}
