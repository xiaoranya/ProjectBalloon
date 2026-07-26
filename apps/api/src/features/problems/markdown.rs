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
    use super::render_safe;

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
}
