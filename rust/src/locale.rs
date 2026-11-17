//! The parts of the user's locale that change what the Java version printed, applied the same
//! way.

use std::ffi::CStr;

/// The glibc locales (language and country, ignoring the encoding) for which Java's
/// `String.format("%3d", id)` printed task ids in `list` with native digits, and the zero of
/// those digits. Every other locale, including those with an `@` modifier such as
/// `sd_IN@devanagari`, used ASCII digits. Measured with OpenJDK 21 on every glibc locale of the
/// languages that have native digits in Java's locale data.
const NATIVE_ZERO_DIGITS: [(&str, char); 34] = [
    ("ar_BH", '\u{660}'),
    ("ar_EG", '\u{660}'),
    ("ar_IN", '\u{660}'),
    ("ar_IQ", '\u{660}'),
    ("ar_JO", '\u{660}'),
    ("ar_KW", '\u{660}'),
    ("ar_LB", '\u{660}'),
    ("ar_OM", '\u{660}'),
    ("ar_QA", '\u{660}'),
    ("ar_SA", '\u{660}'),
    ("ar_SD", '\u{660}'),
    ("ar_SS", '\u{660}'),
    ("ar_SY", '\u{660}'),
    ("ar_YE", '\u{660}'),
    ("as_IN", '\u{9E6}'),
    ("bho_IN", '\u{966}'),
    ("bho_NP", '\u{966}'),
    ("bn_BD", '\u{9E6}'),
    ("bn_IN", '\u{9E6}'),
    ("ckb_IQ", '\u{660}'),
    ("dz_BT", '\u{F20}'),
    ("fa_IR", '\u{6F0}'),
    ("ks_IN", '\u{6F0}'),
    ("mni_IN", '\u{9E6}'),
    ("mr_IN", '\u{966}'),
    ("my_MM", '\u{1040}'),
    ("ne_NP", '\u{966}'),
    ("ps_AF", '\u{6F0}'),
    ("raj_IN", '\u{966}'),
    ("sa_IN", '\u{966}'),
    ("sat_IN", '\u{1C50}'),
    ("sd_IN", '\u{660}'),
    ("sd_PK", '\u{660}'),
    ("ur_IN", '\u{6F0}'),
];

/// Applies the user's locale settings (`LC_ALL`, `LC_*`, `LANG`) to the C library, as the Java
/// launcher does at startup, so system error messages follow `LC_MESSAGES` as Java's did.
/// Returns the zero digit for task ids in `list`, which follows `LC_CTYPE` as Java's did.
///
/// Call it once, at startup, before starting any threads.
pub fn init() -> char {
    // SAFETY: called at startup while the program is single-threaded; the name returned by the
    // second call is copied before anything can change the locale again.
    let name = unsafe {
        libc::setlocale(libc::LC_ALL, c"".as_ptr());
        let name = libc::setlocale(libc::LC_CTYPE, std::ptr::null());
        if name.is_null() {
            return '0';
        }
        CStr::from_ptr(name).to_string_lossy().into_owned()
    };
    zero_digit(&name)
}

/// The zero digit Java used for a locale name as `setlocale` reports it, such as `ar_SA.UTF-8`.
pub fn zero_digit(name: &str) -> char {
    if name.contains('@') {
        return '0';
    }
    let base = name.split('.').next().unwrap_or_default();
    NATIVE_ZERO_DIGITS
        .iter()
        .find(|(locale, _)| *locale == base)
        .map_or('0', |&(_, zero)| zero)
}

/// Writes ASCII digits in `text` with the digits that start at `zero`.
pub fn localize_digits(text: &str, zero: char) -> String {
    if zero == '0' {
        return text.to_owned();
    }
    text.chars()
        .map(|c| match c.to_digit(10) {
            Some(d) => char::from_u32(u32::from(zero) + d).unwrap_or(c),
            None => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_digit_by_locale_name() {
        assert_eq!(zero_digit("ar_SA.UTF-8"), '\u{660}');
        assert_eq!(zero_digit("ar_SA"), '\u{660}');
        assert_eq!(zero_digit("fa_IR.utf8"), '\u{6F0}');
        assert_eq!(zero_digit("ar_MA.UTF-8"), '0');
        assert_eq!(zero_digit("sd_IN.UTF-8@devanagari"), '0');
        assert_eq!(zero_digit("C.UTF-8"), '0');
        assert_eq!(zero_digit("C"), '0');
        assert_eq!(zero_digit("en_US.UTF-8"), '0');
    }

    #[test]
    fn digits_are_mapped_and_padding_kept() {
        assert_eq!(localize_digits(" 12", '\u{660}'), " \u{661}\u{662}");
        assert_eq!(localize_digits("x9", '0'), "x9");
    }
}
