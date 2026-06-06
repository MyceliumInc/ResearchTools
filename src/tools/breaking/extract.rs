use std::collections::HashSet;

const ENTITY_STOPS: &[&str] = &[
    "The", "A", "An", "But", "And", "Or", "For", "In", "On", "At", "To", "Of", "It", "Is",
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
    "January", "February", "March", "April", "May", "June", "July", "August",
    "September", "October", "November", "December",
    "News", "Live", "Breaking", "Update", "Watch", "Read", "How", "Why", "What", "When", "Where",
    "Photos", "Video", "Opinion", "Editorial",
];

const STOPWORDS: &[&str] = &[
    "the","a","an","and","or","but","of","in","on","at","to","for","with","by","from","up","about","into","over","after","is","are","was","were","be","been","being","have","has","had","do","does","did","will","would","could","should","may","might","can","this","that","these","those","you","he","she","it","we","they","what","which","who","whom","whose","when","where","why","how","as","if","than","then","also","just","not","no","so","very","more","most","much","many","some","any","all","each","every","other","another","such","new","says","said","say","its","his","her","their","our","my","your"
];

const SUFFIX_PATTERNS: &[&str] = &[
    " - BBC News", " | BBC News",
    " - The New York Times", " | The New York Times",
    " - The Guardian", " | The Guardian",
    " - NPR", " | NPR",
    " - CNN", " | CNN", " | CNN.com",
    " - Al Jazeera", " | Al Jazeera",
    " - The Washington Post", " | The Washington Post",
    " - Sky News", " | Sky News",
    " – live", " - live", " | live", " - Live Updates", " — live updates",
];

pub fn extract_entities(headline: &str, description: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for text in [headline, description] {
        let bytes = text.as_bytes();
        let length = bytes.len();

        let mut index = 0;
        while index < length {
            if !bytes[index].is_ascii_uppercase() {
                index += 1;
                continue;
            }
            let word_start = index;
            index += 1;
            while index < length && (bytes[index].is_ascii_alphabetic() || bytes[index] == b'\'') {
                index += 1;
            }
            let word = match std::str::from_utf8(&bytes[word_start..index]) {
                Ok(word) => word,
                Err(_) => continue,
            };
            if is_entity_stop(word) {
                continue;
            }
            let all_upper = word.bytes().all(|byte| byte.is_ascii_uppercase() || byte == b'\'');
            if word.len() < 3 && !all_upper {
                continue;
            }
            let lowered = word.to_lowercase();
            if lowered.len() < 2 {
                continue;
            }
            if seen.insert(lowered.clone()) {
                out.push(lowered);
            }
        }

        let mut index = 0;
        while index < length {
            if !bytes[index].is_ascii_digit() {
                index += 1;
                continue;
            }
            let number_start = index;
            while index < length
                && (bytes[index].is_ascii_digit() || bytes[index] == b',' || bytes[index] == b'.')
            {
                index += 1;
            }
            if let Ok(number) = std::str::from_utf8(&bytes[number_start..index]) {
                if number.len() >= 2 {
                    let key = number.to_string();
                    if seen.insert(key.clone()) {
                        out.push(key);
                    }
                }
            }
        }
    }
    out
}

pub fn make_tokens(normalized: &str) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for token in normalized.split_whitespace() {
        if token.len() < 2 || is_stopword(token) {
            continue;
        }
        let stemmed = stem(token);
        if stemmed.len() < 2 {
            continue;
        }
        if seen.insert(stemmed.clone()) {
            out.push(stemmed);
        }
    }
    out
}

pub fn normalize(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut at_boundary = true;
    for character in input.chars() {
        if character.is_alphanumeric() {
            for lowered in character.to_lowercase() {
                output.push(lowered);
            }
            at_boundary = false;
        } else if !at_boundary {
            output.push(' ');
            at_boundary = true;
        }
    }
    output.trim().to_string()
}

pub fn clean_headline(headline: &str) -> String {
    let mut output = headline.to_string();
    for suffix in SUFFIX_PATTERNS {
        if let Some(rest) = output.strip_suffix(suffix) {
            output = rest.to_string();
            break;
        }
    }
    output.trim().to_string()
}

pub fn is_live_blog_url(url: &str) -> bool {
    let lowered = url.to_ascii_lowercase();
    lowered.contains("/live/")
        || lowered.contains("live-updates")
        || lowered.contains("live-news")
        || lowered.contains("liveblog")
}

pub fn jaccard(left: &[String], right: &[String]) -> f32 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let left_set: HashSet<&str> = left.iter().map(String::as_str).collect();
    let right_set: HashSet<&str> = right.iter().map(String::as_str).collect();
    let intersection = left_set.intersection(&right_set).count() as f32;
    let union = left_set.union(&right_set).count() as f32;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn stem(token: &str) -> String {
    let mut stemmed = token.to_string();
    for suffix in ["ies", "ing", "ed", "s"] {
        if stemmed.len() > suffix.len() + 2 && stemmed.ends_with(suffix) {
            stemmed.truncate(stemmed.len() - suffix.len());
            if suffix == "ies" {
                stemmed.push('y');
            }
            return stemmed;
        }
    }
    stemmed
}

fn is_entity_stop(word: &str) -> bool {
    ENTITY_STOPS.contains(&word)
}

fn is_stopword(word: &str) -> bool {
    STOPWORDS.contains(&word)
}
