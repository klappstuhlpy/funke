//! Inline SVG glyph icons for results with no natural system icon (commands, calculator).
//!
//! Pure string assembly: the output is a `data:image/svg+xml` URL that the UI renders
//! exactly like the PNG data URLs the shell produces. Glyphs are stroked outlines in the
//! palette's dim ivory so they stay quieter than full-color app icons.

/// `--text-dim` from the UI palette, with `#` pre-encoded for the data URL.
const STROKE: &str = "%23aaa295";

/// Wrap SVG path elements (drawn in a 24×24 viewbox) into a stroked-outline icon and
/// return it as a data URL for [`ResultItem::icon`](crate::ResultItem::icon).
pub fn glyph_data_url(body: &str) -> String {
    format!(
        "data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none' \
         stroke='{STROKE}' stroke-width='1.7' stroke-linecap='round' stroke-linejoin='round'>{body}</svg>"
    )
    .replace(' ', "%20")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn produces_a_fully_encoded_data_url() {
        let url = glyph_data_url("<path d='M6 9.5h12'/>");
        assert!(url.starts_with("data:image/svg+xml;utf8,"));
        assert!(!url.contains(' '), "spaces must be percent-encoded");
        assert!(!url.contains('#'), "hashes must be percent-encoded");
        assert!(url.contains("M6%209.5h12"));
    }
}
