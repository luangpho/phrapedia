//! Port of shared/utils/slug.ts.

use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

const UNSAFE: &str = "/\\?#%&=+:;,\"'`<>|{}[]()!*^$@~";

/// URL-safe slug. Latin is lower-cased and dashed; Thai / Japanese are kept. Empty when nothing usable remains.
pub fn slugify(input: &str) -> String {
    let normalized: String = input.nfkc().collect();
    let lowered = normalized.trim().to_lowercase();
    // [\s_]+ -> '-'
    let mut s = String::new();
    let mut in_sep = false;
    for c in lowered.chars() {
        if c.is_whitespace() || c == '_' {
            if !in_sep {
                s.push('-');
            }
            in_sep = true;
        } else {
            in_sep = false;
            s.push(c);
        }
    }
    // remove unsafe characters
    let s: String = s.chars().filter(|c| !UNSAFE.contains(*c)).collect();
    // \.+ -> '.'  and  -+ -> '-'
    let mut out = String::new();
    let mut prev: Option<char> = None;
    for c in s.chars() {
        if (c == '.' || c == '-') && prev == Some(c) {
            continue;
        }
        out.push(c);
        prev = Some(c);
    }
    out.trim_matches(|c| c == '-' || c == '.').to_string()
}

pub fn is_valid_slug(slug: &str) -> bool {
    let len = slug.encode_utf16().count();
    len > 0 && len <= 120 && !slug.contains('/') && !slug.contains('\\') && slug != "." && slug != ".." && !slug.starts_with('.')
}

/// Short random base36-ish id used for fallback slugs.
pub fn short_id(len: usize) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let bytes = Uuid::new_v4();
    let b = bytes.as_bytes();
    (0..len).map(|i| ALPHABET[b[i % 16] as usize % ALPHABET.len()] as char).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latin_is_lowercased_and_dashed() {
        assert_eq!(slugify("Luang Pho To"), "luang-pho-to");
        assert_eq!(slugify("  Wat  Rakhang_Khositaram "), "wat-rakhang-khositaram");
    }

    #[test]
    fn thai_and_japanese_are_kept() {
        assert_eq!(slugify("หลวงพ่อโต"), "หลวงพ่อโต");
        assert_eq!(slugify("ルアンポー・トー"), "ルアンポー・トー");
    }

    #[test]
    fn nfkc_and_unsafe_chars() {
        assert_eq!(slugify("ＡＢＣ"), "abc");
        assert_eq!(slugify("a/b\\c?d#e%f&g=h+i:j;k,l\"m'n"), "abcdefghijklmn");
        assert_eq!(slugify("(Wat) [Rakhang]!"), "wat-rakhang");
    }

    #[test]
    fn dots_and_dashes_collapse_and_trim() {
        assert_eq!(slugify("a..b"), "a.b");
        assert_eq!(slugify("--a---b--"), "a-b");
        assert_eq!(slugify(".hidden."), "hidden");
        assert_eq!(slugify("..."), "");
        assert_eq!(slugify(""), "");
    }

    #[test]
    fn validity() {
        assert!(is_valid_slug("somdet-to"));
        assert!(is_valid_slug("หลวงพ่อโต"));
        for bad in ["", ".", "..", ".x", "a/b", "a\\b"] {
            assert!(!is_valid_slug(bad), "{bad}");
        }
        assert!(!is_valid_slug(&"a".repeat(121)));
        assert!(is_valid_slug(&"a".repeat(120)));
    }

    #[test]
    fn short_id_shape() {
        let id = short_id(6);
        assert_eq!(id.len(), 6);
        assert!(id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    }
}
