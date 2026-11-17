//! Reproductions of the Java library behavior the Java version relied on, so that the same
//! input is accepted and rejected, and the same paths are used, as before.

use std::env;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

/// `Character.isWhitespace`, used by Java's `String.strip` and `String.isBlank`. Unlike
/// `char::is_whitespace` it excludes the no-break spaces (U+00A0, U+2007, U+202F) and U+0085,
/// and includes the separators U+001C to U+001F.
pub fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        '\t'..='\r'
            | '\u{1C}'..='\u{20}'
            | '\u{1680}'
            | '\u{2000}'..='\u{2006}'
            | '\u{2008}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{205F}'
            | '\u{3000}'
    )
}

/// Java's `String.strip`.
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_whitespace)
}

/// Java's `String.isBlank`.
pub fn is_blank(s: &str) -> bool {
    s.chars().all(is_whitespace)
}

/// The zero of every run of ten decimal digits that Java's `Character.digit(char, 10)` accepts.
/// Java parses numbers one UTF-16 unit at a time, so digits outside the Basic Multilingual Plane
/// are never accepted.
const DIGIT_ZEROS: [u32; 37] = [
    0x0030, 0x0660, 0x06F0, 0x07C0, 0x0966, 0x09E6, 0x0A66, 0x0AE6, 0x0B66, 0x0BE6, 0x0C66, 0x0CE6,
    0x0D66, 0x0DE6, 0x0E50, 0x0ED0, 0x0F20, 0x1040, 0x1090, 0x17E0, 0x1810, 0x1946, 0x19D0, 0x1A80,
    0x1A90, 0x1B50, 0x1BB0, 0x1C40, 0x1C50, 0xA620, 0xA8D0, 0xA900, 0xA9D0, 0xA9F0, 0xAA50, 0xABF0,
    0xFF10,
];

fn digit(c: char) -> Option<u32> {
    let c = u32::from(c);
    DIGIT_ZEROS
        .iter()
        .find(|&&zero| (zero..zero + 10).contains(&c))
        .map(|zero| c - zero)
}

fn parse_decimal(s: &str, min: i64, max: i64) -> Option<i64> {
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    if digits.is_empty() {
        return None;
    }
    let limit = i128::from(max) + 1;
    let mut value: i128 = 0;
    for c in digits.chars() {
        value = value * 10 + i128::from(digit(c)?);
        if value > limit {
            return None;
        }
    }
    let value = if negative { -value } else { value };
    (i128::from(min)..=i128::from(max))
        .contains(&value)
        .then_some(value as i64)
}

/// Java's `Integer.parseInt`: an optional `+` or `-`, then decimal digits from any script.
pub fn parse_int(s: &str) -> Option<i32> {
    parse_decimal(s, i32::MIN.into(), i32::MAX.into()).map(|v| v as i32)
}

/// Java's `Long.parseLong`.
pub fn parse_long(s: &str) -> Option<i64> {
    parse_decimal(s, i64::MIN, i64::MAX)
}

/// Splits text into lines the way Java's `BufferedReader.readLine` does: at `\n`, `\r` or
/// `\r\n`, with no empty line after a final line break.
pub fn lines(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let (line, next) = match rest.find(['\n', '\r']) {
            None => (rest, ""),
            Some(i) => {
                let skip = if rest[i..].starts_with("\r\n") { 2 } else { 1 };
                (&rest[..i], &rest[i + skip..])
            }
        };
        rest = next;
        Some(line)
    })
}

// Java's Unix paths drop repeated and trailing slashes, and compute parents, file names and
// absolute paths on the text of the path without touching "." or ".." components. These
// helpers do the same so that the same files are used and the same paths appear in messages.

/// Java's `Path.of(path)`: removes repeated slashes and a trailing slash.
pub fn normalize(path: &OsStr) -> PathBuf {
    let mut out: Vec<u8> = Vec::with_capacity(path.len());
    for &b in path.as_bytes() {
        if b == b'/' && out.last() == Some(&b'/') {
            continue;
        }
        out.push(b);
    }
    if out.len() > 1 && out.last() == Some(&b'/') {
        out.pop();
    }
    PathBuf::from(OsString::from_vec(out))
}

/// Java's `Path.of(first, more)` for two parts.
pub fn join(first: &OsStr, second: &OsStr) -> PathBuf {
    let mut joined = first.to_os_string();
    if !second.is_empty() {
        if !joined.is_empty() {
            joined.push("/");
        }
        joined.push(second);
    }
    normalize(&joined)
}

fn is_absolute(path: &Path) -> bool {
    path.as_os_str().as_bytes().first() == Some(&b'/')
}

/// Java's `Path.resolve`.
pub fn resolve(base: &Path, other: &Path) -> PathBuf {
    let base_bytes = base.as_os_str().as_bytes();
    if is_absolute(other) || base_bytes.is_empty() {
        return other.to_path_buf();
    }
    if other.as_os_str().is_empty() {
        return base.to_path_buf();
    }
    let mut joined = base.as_os_str().to_os_string();
    if base_bytes.last() != Some(&b'/') {
        joined.push("/");
    }
    joined.push(other.as_os_str());
    PathBuf::from(joined)
}

/// Java's `Path.getParent`: `None` for `/` and for a single relative name.
pub fn parent(path: &Path) -> Option<PathBuf> {
    let bytes = path.as_os_str().as_bytes();
    match bytes.iter().rposition(|&b| b == b'/') {
        None => None,
        Some(0) if bytes.len() == 1 => None,
        Some(0) => Some(PathBuf::from("/")),
        Some(i) => Some(PathBuf::from(OsStr::from_bytes(&bytes[..i]))),
    }
}

/// Java's `Path.getFileName`: the last name, or `None` for `/`. Unlike `Path::file_name` it
/// returns `.` and `..` as names.
pub fn file_name(path: &Path) -> Option<&OsStr> {
    let bytes = path.as_os_str().as_bytes();
    let name = match bytes.iter().rposition(|&b| b == b'/') {
        None => bytes,
        Some(i) => &bytes[i + 1..],
    };
    (!name.is_empty()).then(|| OsStr::from_bytes(name))
}

/// Java's `Path.resolveSibling`.
pub fn resolve_sibling(path: &Path, other: &Path) -> PathBuf {
    match parent(path) {
        Some(parent) => resolve(&parent, other),
        None => other.to_path_buf(),
    }
}

/// Java's `Path.toAbsolutePath`: prefixes the working directory without normalizing.
pub fn to_absolute(path: &Path) -> PathBuf {
    if is_absolute(path) {
        return path.to_path_buf();
    }
    match env::current_dir() {
        Ok(cwd) => resolve(&cwd, path),
        Err(_) => path.to_path_buf(),
    }
}

/// Decodes bytes the way Java's `new String(bytes, UTF_8)` does, which is how the Java launcher
/// decodes command-line arguments under a UTF-8 locale. Valid UTF-8 decodes as usual; each
/// malformed sequence becomes U+FFFD, grouped as Java groups them (an encoded UTF-16 surrogate
/// is one replacement, where `String::from_utf8_lossy` gives three).
pub fn decode_utf8_lossy(src: &[u8]) -> String {
    const REPL: char = '\u{FFFD}';
    let is_continuation = |b: u8| b & 0xC0 == 0x80;
    let mut out = String::with_capacity(src.len());
    let len = src.len();
    let mut sp = 0;
    while sp < len {
        let b1 = src[sp];
        sp += 1;
        if b1 < 0x80 {
            out.push(char::from(b1));
        } else if b1 >> 5 == 0b110 && b1 & 0x1E != 0 {
            if sp == len {
                out.push(REPL);
                break;
            }
            let b2 = src[sp];
            if is_continuation(b2) {
                sp += 1;
                out.push(decode(&[b1 & 0x1F, b2 & 0x3F]));
            } else {
                out.push(REPL);
            }
        } else if b1 >> 4 == 0b1110 {
            let overlong = |b2: u8| b1 == 0xE0 && b2 & 0xE0 == 0x80;
            if sp + 1 < len {
                let (b2, b3) = (src[sp], src[sp + 1]);
                if overlong(b2) || !is_continuation(b2) || !is_continuation(b3) {
                    out.push(REPL);
                    sp += if overlong(b2) || !is_continuation(b2) {
                        0
                    } else {
                        1
                    };
                } else {
                    sp += 2;
                    let c = u32::from(b1 & 0x0F) << 12
                        | u32::from(b2 & 0x3F) << 6
                        | u32::from(b3 & 0x3F);
                    out.push(char::from_u32(c).unwrap_or(REPL));
                }
                continue;
            }
            out.push(REPL);
            if sp < len && (overlong(src[sp]) || !is_continuation(src[sp])) {
                continue;
            }
            break;
        } else if b1 >> 3 == 0b11110 {
            let bad_second = |b2: u8| {
                b1 > 0xF4
                    || (b1 == 0xF0 && !(0x90..=0xBF).contains(&b2))
                    || (b1 == 0xF4 && b2 & 0xF0 != 0x80)
                    || !is_continuation(b2)
            };
            if sp + 2 < len {
                let (b2, b3, b4) = (src[sp], src[sp + 1], src[sp + 2]);
                let well_formed = is_continuation(b2) && is_continuation(b3) && is_continuation(b4);
                let c = u32::from(b1 & 0x07) << 18
                    | u32::from(b2 & 0x3F) << 12
                    | u32::from(b3 & 0x3F) << 6
                    | u32::from(b4 & 0x3F);
                if well_formed && (0x10000..=0x10FFFF).contains(&c) {
                    sp += 3;
                    out.push(char::from_u32(c).unwrap_or(REPL));
                } else {
                    out.push(REPL);
                    sp += if bad_second(b2) {
                        0
                    } else if !is_continuation(b3) {
                        1
                    } else {
                        2
                    };
                }
                continue;
            }
            out.push(REPL);
            if b1 > 0xF4 || (sp < len && bad_second(src[sp])) {
                continue;
            }
            sp += 1;
            if sp < len && !is_continuation(src[sp]) {
                continue;
            }
            break;
        } else {
            out.push(REPL);
        }
    }
    out
}

fn decode(parts: &[u8; 2]) -> char {
    char::from_u32(u32::from(parts[0]) << 6 | u32::from(parts[1])).unwrap_or('\u{FFFD}')
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whitespace_follows_java_rules() {
        assert_eq!(strip("\u{1F} x \t\n"), "x");
        assert_eq!(strip("\u{A0}x\u{A0}"), "\u{A0}x\u{A0}");
        assert_eq!(strip("x\u{85}"), "x\u{85}");
        assert_eq!(strip("\u{3000}x\u{2007}"), "x\u{2007}");
        assert!(is_blank(""));
        assert!(is_blank(" \u{B}\u{C}\u{1C}"));
        assert!(!is_blank("\u{A0}"));
    }

    #[test]
    fn parse_int_matches_java() {
        assert_eq!(parse_int("42"), Some(42));
        assert_eq!(parse_int("+7"), Some(7));
        assert_eq!(parse_int("-3"), Some(-3));
        assert_eq!(parse_int("007"), Some(7));
        assert_eq!(parse_int("\u{661}\u{662}"), Some(12));
        assert_eq!(parse_int("\u{FF15}"), Some(5));
        assert_eq!(parse_int("2147483647"), Some(i32::MAX));
        assert_eq!(parse_int("-2147483648"), Some(i32::MIN));
        assert_eq!(parse_int("2147483648"), None);
        assert_eq!(parse_int("99999999999999999999999"), None);
        for bad in [
            "",
            "+",
            "-",
            " 1",
            "1 ",
            "1.0",
            "0x1",
            "+-1",
            "\u{1D7CE}",
            "½",
            "1_000",
        ] {
            assert_eq!(parse_int(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn parse_long_range() {
        assert_eq!(parse_long("9223372036854775807"), Some(i64::MAX));
        assert_eq!(parse_long("-9223372036854775808"), Some(i64::MIN));
        assert_eq!(parse_long("9223372036854775808"), None);
    }

    #[test]
    fn lines_split_like_buffered_reader() {
        let split = |s| lines(s).collect::<Vec<_>>();
        assert_eq!(split(""), Vec::<&str>::new());
        assert_eq!(split("\n"), [""]);
        assert_eq!(split("a\nb"), ["a", "b"]);
        assert_eq!(split("a\r\nb\rc\n"), ["a", "b", "c"]);
        assert_eq!(split("a\r\r\nb"), ["a", "", "b"]);
    }

    #[test]
    fn decoding_valid_utf8_is_unchanged() {
        let text = "plain ascii, café ✓, 日本語, 🎉";
        assert_eq!(decode_utf8_lossy(text.as_bytes()), text);
        assert_eq!(decode_utf8_lossy(b"a\xed\xa0\x80b"), "a\u{FFFD}b");
        assert_eq!(decode_utf8_lossy(b"\xff\xfe"), "\u{FFFD}\u{FFFD}");
    }

    #[test]
    fn paths_follow_java_rules() {
        assert_eq!(normalize("/a//b/".as_ref()), Path::new("/a/b"));
        assert_eq!(normalize("//".as_ref()), Path::new("/"));
        assert_eq!(normalize("a/./b".as_ref()), Path::new("a/./b"));
        assert_eq!(
            join("/".as_ref(), ".todo.tsv".as_ref()),
            Path::new("/.todo.tsv")
        );
        assert_eq!(
            join("".as_ref(), ".todo.tsv".as_ref()),
            Path::new(".todo.tsv")
        );
        assert_eq!(parent(Path::new("/a/.")), Some(PathBuf::from("/a")));
        assert_eq!(parent(Path::new("/a")), Some(PathBuf::from("/")));
        assert_eq!(parent(Path::new("/")), None);
        assert_eq!(parent(Path::new("a")), None);
        assert_eq!(file_name(Path::new("/a/..")), Some(OsStr::new("..")));
        assert_eq!(file_name(Path::new("/")), None);
        assert_eq!(
            resolve_sibling(Path::new("/a/link"), Path::new("b/c")),
            Path::new("/a/b/c")
        );
        assert_eq!(
            resolve_sibling(Path::new("/a/link"), Path::new("/x")),
            Path::new("/x")
        );
        assert_eq!(resolve(Path::new("/"), Path::new("x")), Path::new("/x"));
    }
}
