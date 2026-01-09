use regex::Regex;
use std::sync::OnceLock;

static PUNCTUATION_PATTERNS: OnceLock<Vec<(Regex, &'static str)>> = OnceLock::new();

fn get_patterns() -> &'static Vec<(Regex, &'static str)> {
	PUNCTUATION_PATTERNS.get_or_init(|| {
		[
			("question mark", "?"),
			("exclamation mark", "!"),
			("exclamation point", "!"),
			("open paren", "("),
			("close paren", ")"),
			("open bracket", "["),
			("close bracket", "]"),
			("open brace", "{"),
			("close brace", "}"),
			("new paragraph", "\n\n"),
			("new line", "\n"),
			("period", "."),
			("comma", ","),
			("colon", ":"),
			("semicolon", ";"),
			("dash", "-"),
			("hyphen", "-"),
			("underscore", "_"),
			("hash", "#"),
			("asterisk", "*"),
			("slash", "/"),
			("backslash", "\\"),
			("pipe", "|"),
			("tilde", "~"),
			("tab", "\t"),
		]
		.into_iter()
		.filter_map(|(phrase, symbol)| {
			let pattern = format!(r"(?i)\b{}\b", regex::escape(phrase));
			Regex::new(&pattern).ok().map(|re| (re, symbol))
		})
		.collect()
	})
}

pub fn process_punctuation(text: &str) -> String {
	let mut result = text.to_string();
	for (re, symbol) in get_patterns() {
		result = re.replace_all(&result, *symbol).into_owned();
	}
	for punct in ['.', ',', '?', '!', ':', ';', ')', ']', '}'] {
		result = result.replace(&format!(" {}", punct), &punct.to_string());
	}
	for punct in ['(', '[', '{', '#', '@'] {
		result = result.replace(&format!("{} ", punct), &punct.to_string());
	}
	capitalize_sentences(&result)
}

fn capitalize_sentences(text: &str) -> String {
	let mut result = String::with_capacity(text.len());
	let mut capitalize_next = true;
	let mut last_was_sentence_end = false;
	for c in text.chars() {
		if last_was_sentence_end && c.is_alphabetic() {
			result.push(' ');
		}
		if capitalize_next && c.is_alphabetic() {
			result.extend(c.to_uppercase());
			capitalize_next = false;
			last_was_sentence_end = false;
		} else {
			result.push(c);
			if c == '.' || c == '!' || c == '?' {
				capitalize_next = true;
				last_was_sentence_end = true;
			} else if c == '\n' {
				capitalize_next = true;
				last_was_sentence_end = false;
			} else if c.is_whitespace() {
				last_was_sentence_end = false;
			} else {
				capitalize_next = false;
				last_was_sentence_end = false;
			}
		}
	}
	result
}
