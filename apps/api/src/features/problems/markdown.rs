use ammonia::Builder;
use pulldown_cmark::{Options, Parser, html};

#[must_use]
pub fn render_safe(source: &str) -> String {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS;
    let mut rendered = String::new();
    html::push_html(&mut rendered, Parser::new_ext(source, options));
    Builder::default().clean(&rendered).to_string()
}

#[cfg(test)]
mod tests {
    use crate::features::problems::markdown::render_safe;

    #[test]
    fn dangerous_html_and_urls_are_removed() {
        let rendered = render_safe(
            r#"# Sum

<script>alert(1)</script>
<img src=x onerror="alert(2)">
[bad](javascript:alert(3))
<a href="javascript:alert(4)">also bad</a>
[safe](https://example.invalid/problem)
"#,
        );

        assert!(rendered.contains("<h1>Sum</h1>"));
        assert!(!rendered.contains("<script"));
        assert!(!rendered.contains("onerror"));
        assert!(!rendered.contains("href=\"javascript:"));
        assert!(rendered.contains("https://example.invalid/problem"));
    }

    #[test]
    fn dangerous_embed_elements_are_removed() {
        let rendered = render_safe(
            r#"# Sum

<iframe src="https://evil.invalid"></iframe>
<svg onload="alert(1)"><circle r="1"></circle></svg>
<style>body { display: none }</style>
"#,
        );

        assert!(!rendered.contains("<iframe"));
        assert!(!rendered.contains("<svg"));
        assert!(!rendered.contains("<style"));
    }

    #[test]
    fn exotic_url_schemes_are_stripped() {
        let rendered = render_safe(
            r#"[data](data:text/html;base64,PHNjcmlwdD4=)
[vbscript](vbscript:msgbox(1))
[mixed case](JaVaScRiPt:alert(1))
[entity](javascript&#58;alert(1))
[file](file:///etc/passwd)
"#,
        );

        assert!(!rendered.contains("data:text/html"));
        assert!(!rendered.contains("vbscript:"));
        assert!(!rendered.contains("file://"));
        assert!(!rendered.contains("alert(1)"), "scheme payload survived: {rendered}");
    }

    #[test]
    fn rendered_links_carry_noopener_rel() {
        let rendered = render_safe("[site](https://example.invalid/page)");
        assert!(
            rendered.contains("rel=\"noopener noreferrer\""),
            "links must keep the noopener rel: {rendered}"
        );
    }
}
