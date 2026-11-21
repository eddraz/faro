//! HTML content extraction and normalization into clean Markdown or plain text.

use anyhow::{anyhow, Result};
use scraper::{ElementRef, Html, Node, Selector};

/// Target format for extracted page content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExtractFormat {
    Markdown,
    Text,
}

/// Extracted document title and normalized body content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExtractedPage {
    pub(crate) title: String,
    pub(crate) content: String,
}

const PRIORITY_CONTENT_SELECTORS: &[&str] = &[
    "main",
    "[role=\"main\"]",
    "article",
    ".markdown-body",
    ".md-content",
    "#content",
    ".content",
];

const NOISE_TAGS: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "canvas", "iframe", "nav", "footer",
    "aside", "form", "button", "input", "select", "textarea",
];

const NOISE_ROLES: &[&str] = &[
    "navigation",
    "banner",
    "contentinfo",
    "complementary",
    "search",
];

/// Extract the main readable content from `html` as clean Markdown or plain text.
///
/// `heading_offset` shifts Markdown headings (`h1`..`h6`) down by N levels,
/// clamped at `h6`, so multi-page outputs nest cleanly under per-source headers.
pub(crate) fn extract_page(
    html: &str,
    base_url: &str,
    format: ExtractFormat,
    selector: Option<&str>,
    heading_offset: u8,
) -> Result<ExtractedPage> {
    let document = Html::parse_document(html);
    let title = extract_title(&document);
    let roots = select_roots(&document, selector)?;

    let mut ctx = RenderCtx {
        base_url,
        format,
        heading_offset,
        in_pre: false,
        in_heading: false,
        blockquote_depth: 0,
        list_stack: Vec::new(),
    };

    let mut raw = String::new();
    for root in roots {
        walk_element(root, &mut ctx, &mut raw);
        ensure_blank_line(&mut raw);
    }

    let mut content = normalize_output(&raw);
    if format == ExtractFormat::Markdown
        && heading_offset == 0
        && !title.is_empty()
        && !content.is_empty()
        && !starts_with_h1(&content)
    {
        content = format!("# {title}\n\n{content}");
    }

    Ok(ExtractedPage { title, content })
}

fn starts_with_h1(markdown: &str) -> bool {
    markdown
        .lines()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.starts_with("# "))
}

fn extract_title(document: &Html) -> String {
    if let Ok(sel) = Selector::parse("title") {
        if let Some(el) = document.select(&sel).next() {
            let text = collapse_ws(&el.text().collect::<String>());
            if !text.is_empty() {
                return text;
            }
        }
    }
    if let Ok(sel) = Selector::parse("h1") {
        if let Some(el) = document.select(&sel).next() {
            let text = collapse_ws(&el.text().collect::<String>());
            if !text.is_empty() {
                return text;
            }
        }
    }
    String::new()
}

fn select_roots<'a>(document: &'a Html, selector: Option<&str>) -> Result<Vec<ElementRef<'a>>> {
    if let Some(css) = selector {
        let parsed =
            Selector::parse(css).map_err(|e| anyhow!("invalid CSS selector {css:?}: {e:?}"))?;
        let matched: Vec<ElementRef<'a>> = document.select(&parsed).collect();
        if matched.is_empty() {
            return Err(anyhow!("CSS selector {css:?} matched no elements"));
        }
        return Ok(matched);
    }

    for candidate in PRIORITY_CONTENT_SELECTORS {
        if let Ok(sel) = Selector::parse(candidate) {
            let matched: Vec<ElementRef<'a>> = document.select(&sel).collect();
            if !matched.is_empty() && visible_text_len(&matched) >= 40 {
                return Ok(matched);
            }
        }
    }

    if let Ok(body_sel) = Selector::parse("body") {
        let body: Vec<ElementRef<'a>> = document.select(&body_sel).collect();
        if !body.is_empty() {
            return Ok(body);
        }
    }

    Ok(vec![document.root_element()])
}

fn visible_text_len(elements: &[ElementRef<'_>]) -> usize {
    elements
        .iter()
        .map(|el| {
            let mut len = 0usize;
            for node in el.descendants() {
                if let Some(parent_el) = node.parent().and_then(ElementRef::wrap) {
                    if is_noise_or_inside_noise(parent_el) {
                        continue;
                    }
                }
                if let Node::Text(t) = node.value() {
                    len += t.trim().len();
                }
            }
            len
        })
        .sum()
}

fn is_noise_or_inside_noise(mut el: ElementRef<'_>) -> bool {
    loop {
        if is_noise_element(el) {
            return true;
        }
        match el.parent().and_then(ElementRef::wrap) {
            Some(parent) => el = parent,
            None => return false,
        }
    }
}

fn is_noise_element(el: ElementRef<'_>) -> bool {
    let val = el.value();
    let tag = val.name();
    if NOISE_TAGS.contains(&tag) {
        return true;
    }
    if val
        .attr("aria-hidden")
        .is_some_and(|v| v.eq_ignore_ascii_case("true"))
    {
        return true;
    }
    if val
        .attr("role")
        .is_some_and(|r| NOISE_ROLES.iter().any(|nr| r.eq_ignore_ascii_case(nr)))
    {
        return true;
    }
    false
}

#[derive(Clone, Copy, Debug)]
enum ListKind {
    Unordered,
    Ordered(usize),
}

struct RenderCtx<'a> {
    base_url: &'a str,
    format: ExtractFormat,
    heading_offset: u8,
    in_pre: bool,
    in_heading: bool,
    blockquote_depth: usize,
    list_stack: Vec<ListKind>,
}

fn walk_children(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    for child in el.children() {
        match child.value() {
            Node::Text(text) => {
                if ctx.in_pre {
                    out.push_str(text);
                    continue;
                }
                let collapsed = collapse_ws(text);
                if collapsed.is_empty() {
                    continue;
                }
                push_prose_text(out, &collapsed);
            }
            Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    walk_element(child_el, ctx, out);
                }
            }
            _ => {}
        }
    }
}

fn walk_element(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    if is_noise_element(el) {
        return;
    }

    let tag = el.value().name();
    match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => render_heading(el, tag, ctx, out),
        "p" | "section" | "article" | "main" => {
            ensure_blank_line(out);
            walk_children(el, ctx, out);
            ensure_blank_line(out);
        }
        "div" => {
            let needs_break = !out.is_empty() && !out.ends_with('\n');
            if needs_break {
                out.push('\n');
            }
            walk_children(el, ctx, out);
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
        }
        "br" => out.push('\n'),
        "hr" => {
            ensure_blank_line(out);
            if ctx.format == ExtractFormat::Markdown {
                out.push_str("---\n\n");
            }
        }
        "pre" => render_pre(el, ctx, out),
        "code" if !ctx.in_pre => render_inline_code(el, ctx, out),
        "strong" | "b" => render_wrapped_inline(el, "**", ctx, out),
        "em" | "i" => render_wrapped_inline(el, "*", ctx, out),
        "a" => render_link(el, ctx, out),
        "img" => render_image(el, ctx, out),
        "ul" => render_list(el, ListKind::Unordered, ctx, out),
        "ol" => render_list(el, ListKind::Ordered(1), ctx, out),
        "li" => render_list_item(el, ctx, out),
        "blockquote" => render_blockquote(el, ctx, out),
        "table" => render_table(el, ctx, out),
        _ => walk_children(el, ctx, out),
    }
}

fn render_heading(el: ElementRef<'_>, tag: &str, ctx: &mut RenderCtx<'_>, out: &mut String) {
    let base_level = tag[1..].parse::<u8>().unwrap_or(1);
    let level = base_level.saturating_add(ctx.heading_offset).min(6);

    let prev_heading = ctx.in_heading;
    ctx.in_heading = true;
    let mut inner = String::new();
    walk_children(el, ctx, &mut inner);
    ctx.in_heading = prev_heading;

    let clean = clean_heading_text(&inner);
    if clean.is_empty() {
        return;
    }

    ensure_blank_line(out);
    if ctx.format == ExtractFormat::Markdown {
        for _ in 0..level {
            out.push('#');
        }
        out.push(' ');
    }
    out.push_str(&clean);
    out.push_str("\n\n");
}

fn clean_heading_text(raw: &str) -> String {
    let collapsed = collapse_ws(raw);
    let trimmed = collapsed
        .trim()
        .trim_end_matches(['¶', '§'])
        .trim()
        .trim_end_matches('#')
        .trim();
    trimmed.to_string()
}

fn render_pre(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    let lang = detect_code_language(el);
    let mut code = String::new();
    collect_pre_text(el, &mut code);
    let code = code.trim_matches('\n').trim_end();
    if code.is_empty() {
        return;
    }

    ensure_blank_line(out);
    if ctx.format == ExtractFormat::Text {
        out.push_str(code);
        out.push_str("\n\n");
        return;
    }

    let fence = safe_fence(code);
    out.push_str(&fence);
    if let Some(lang) = lang {
        out.push_str(&lang);
    }
    out.push('\n');
    out.push_str(code);
    out.push('\n');
    out.push_str(&fence);
    out.push_str("\n\n");
}

fn collect_pre_text(el: ElementRef<'_>, out: &mut String) {
    for child in el.children() {
        match child.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(_) => {
                if let Some(child_el) = ElementRef::wrap(child) {
                    if !is_noise_element(child_el) {
                        collect_pre_text(child_el, out);
                    }
                }
            }
            _ => {}
        }
    }
}

fn detect_code_language(pre: ElementRef<'_>) -> Option<String> {
    let mut candidates = vec![pre];
    if let Ok(sel) = Selector::parse("code") {
        if let Some(code_el) = pre.select(&sel).next() {
            candidates.insert(0, code_el);
        }
    }
    for el in candidates {
        if let Some(classes) = el.value().attr("class") {
            for cls in classes.split_whitespace() {
                if let Some(lang) = cls
                    .strip_prefix("language-")
                    .or_else(|| cls.strip_prefix("lang-"))
                {
                    let clean: String = lang
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                        .collect();
                    if !clean.is_empty() && clean != "plaintext" && clean != "none" {
                        return Some(clean);
                    }
                }
            }
        }
    }
    None
}

fn safe_fence(code: &str) -> String {
    let mut max_run = 2usize;
    let mut cur = 0usize;
    for ch in code.chars() {
        if ch == '`' {
            cur += 1;
            max_run = max_run.max(cur);
        } else {
            cur = 0;
        }
    }
    "`".repeat((max_run + 1).max(3))
}

fn render_inline_code(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    let text = collapse_ws(&el.text().collect::<String>());
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    if ctx.format == ExtractFormat::Text {
        push_prose_text(out, trimmed);
        return;
    }
    let delim = if trimmed.contains('`') { "`` " } else { "`" };
    let end_delim = if trimmed.contains('`') { " ``" } else { "`" };
    ensure_space_before_inline(out, text.starts_with(' '));
    out.push_str(delim);
    out.push_str(trimmed);
    out.push_str(end_delim);
    if text.ends_with(' ') {
        out.push(' ');
    }
}

fn render_wrapped_inline(
    el: ElementRef<'_>,
    marker: &str,
    ctx: &mut RenderCtx<'_>,
    out: &mut String,
) {
    let mut inner = String::new();
    walk_children(el, ctx, &mut inner);
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return;
    }
    if ctx.format == ExtractFormat::Text || ctx.in_heading {
        ensure_space_before_inline(out, inner.starts_with(' '));
        out.push_str(trimmed);
        if inner.ends_with(' ') {
            out.push(' ');
        }
        return;
    }
    ensure_space_before_inline(out, inner.starts_with(' '));
    out.push_str(marker);
    out.push_str(trimmed);
    out.push_str(marker);
    if inner.ends_with(' ') {
        out.push(' ');
    }
}

fn render_link(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    let mut label_buf = String::new();
    walk_children(el, ctx, &mut label_buf);
    let label = label_buf.trim();
    if label.is_empty() {
        return;
    }

    ensure_space_before_inline(out, label_buf.starts_with(' '));
    if ctx.format == ExtractFormat::Text || ctx.in_heading {
        out.push_str(label);
        if label_buf.ends_with(' ') {
            out.push(' ');
        }
        return;
    }

    let href = el.value().attr("href").map(str::trim).unwrap_or("");
    let resolved = resolve_url(ctx.base_url, href);
    if resolved.is_empty() {
        out.push_str(label);
    } else {
        out.push('[');
        out.push_str(label);
        out.push_str("](");
        out.push_str(&resolved);
        out.push(')');
    }
    if label_buf.ends_with(' ') {
        out.push(' ');
    }
}

fn render_image(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    let alt = el
        .value()
        .attr("alt")
        .map(collapse_ws)
        .unwrap_or_default();
    let alt = alt.trim();
    if alt.is_empty() {
        return;
    }
    if ctx.format == ExtractFormat::Text {
        push_prose_text(out, alt);
        return;
    }
    let src = el.value().attr("src").map(str::trim).unwrap_or("");
    let resolved = resolve_url(ctx.base_url, src);
    ensure_space_before_inline(out, false);
    if resolved.is_empty() {
        out.push_str(alt);
    } else {
        out.push_str("![");
        out.push_str(alt);
        out.push_str("](");
        out.push_str(&resolved);
        out.push(')');
    }
}

fn render_list(el: ElementRef<'_>, kind: ListKind, ctx: &mut RenderCtx<'_>, out: &mut String) {
    let is_top = ctx.list_stack.is_empty();
    if is_top {
        ensure_blank_line(out);
    } else if !out.ends_with('\n') {
        out.push('\n');
    }

    ctx.list_stack.push(kind);
    walk_children(el, ctx, out);
    ctx.list_stack.pop();

    if is_top {
        ensure_blank_line(out);
    }
}

fn render_list_item(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    let depth = ctx.list_stack.len().saturating_sub(1);
    for _ in 0..depth {
        out.push_str("  ");
    }

    let prefix = match ctx.list_stack.last_mut() {
        Some(ListKind::Ordered(n)) => {
            let current = *n;
            *n += 1;
            format!("{current}. ")
        }
        _ => "- ".to_string(),
    };
    out.push_str(&prefix);

    let mut item_buf = String::new();
    walk_children(el, ctx, &mut item_buf);
    let trimmed = item_buf.trim();
    out.push_str(trimmed);
    out.push('\n');
}

fn render_blockquote(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    ensure_blank_line(out);
    ctx.blockquote_depth += 1;
    let mut inner = String::new();
    walk_children(el, ctx, &mut inner);
    ctx.blockquote_depth -= 1;

    let clean = normalize_output(&inner);
    if clean.is_empty() {
        return;
    }

    for line in clean.lines() {
        if ctx.format == ExtractFormat::Markdown {
            out.push_str("> ");
        }
        out.push_str(line);
        out.push('\n');
    }
    out.push('\n');
}

fn render_table(el: ElementRef<'_>, ctx: &mut RenderCtx<'_>, out: &mut String) {
    let Ok(tr_sel) = Selector::parse("tr") else {
        return;
    };
    let Ok(cell_sel) = Selector::parse("th, td") else {
        return;
    };

    let mut rows: Vec<Vec<String>> = Vec::new();
    for tr in el.select(&tr_sel) {
        let mut cells = Vec::new();
        for cell in tr.select(&cell_sel) {
            let mut buf = String::new();
            walk_children(cell, ctx, &mut buf);
            let cleaned = collapse_ws(&buf).trim().replace('|', "\\|");
            cells.push(cleaned);
        }
        if cells.iter().any(|c| !c.is_empty()) {
            rows.push(cells);
        }
    }

    if rows.is_empty() {
        return;
    }

    ensure_blank_line(out);
    if ctx.format == ExtractFormat::Text {
        for row in &rows {
            out.push_str(&row.join("\t"));
            out.push('\n');
        }
        out.push('\n');
        return;
    }

    let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
    for (idx, row) in rows.iter().enumerate() {
        out.push('|');
        for col in 0..col_count {
            let cell = row.get(col).map(String::as_str).unwrap_or("");
            out.push(' ');
            out.push_str(cell);
            out.push_str(" |");
        }
        out.push('\n');
        if idx == 0 {
            out.push('|');
            for _ in 0..col_count {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out.push('\n');
}

/// Resolve a relative URL against `base_url`, ignoring `javascript:` URIs.
pub(crate) fn resolve_url(base_url: &str, href: &str) -> String {
    let href = href.trim();
    if href.is_empty() || href == "#" {
        return String::new();
    }
    let lower = href.to_ascii_lowercase();
    if lower.starts_with("javascript:") || lower.starts_with("data:") {
        return String::new();
    }
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return href.to_string();
    }
    if href.starts_with('#') {
        return href.to_string();
    }

    let Some((scheme, rest)) = base_url.split_once("://") else {
        return href.to_string();
    };
    if href.starts_with("//") {
        return format!("{scheme}:{href}");
    }
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() {
        return href.to_string();
    }
    if href.starts_with('/') {
        return format!("{scheme}://{host}{href}");
    }

    let path_without_query = base_url.split(['?', '#']).next().unwrap_or(base_url);
    match path_without_query.rfind('/') {
        Some(idx) if idx > scheme.len() + 2 => {
            format!("{}/{href}", &path_without_query[..idx])
        }
        _ => format!("{scheme}://{host}/{href}"),
    }
}

fn collapse_ws(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_ws = false;
    for ch in input.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                out.push(' ');
                prev_ws = true;
            }
        } else {
            out.push(ch);
            prev_ws = false;
        }
    }
    out
}

fn push_prose_text(out: &mut String, text: &str) {
    let at_line_start = out.is_empty() || out.ends_with('\n');
    let trimmed = if at_line_start {
        text.trim_start()
    } else if out.ends_with(' ') && text.starts_with(' ') {
        &text[1..]
    } else {
        text
    };
    out.push_str(trimmed);
}

fn ensure_space_before_inline(out: &mut String, leading_space: bool) {
    if leading_space && !out.is_empty() && !out.ends_with([' ', '\n', '\t', '(', '[']) {
        out.push(' ');
    }
}

fn ensure_blank_line(out: &mut String) {
    while out.ends_with(' ') {
        out.pop();
    }
    if out.is_empty() {
        return;
    }
    if out.ends_with("\n\n") {
        return;
    }
    if out.ends_with('\n') {
        out.push('\n');
    } else {
        out.push_str("\n\n");
    }
}

fn normalize_output(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut blank_run = 0usize;
    for line in raw.lines() {
        let trimmed_end = line.trim_end();
        if trimmed_end.is_empty() {
            blank_run += 1;
            if blank_run <= 1 && !out.is_empty() {
                out.push('\n');
            }
        } else {
            blank_run = 0;
            out.push_str(trimmed_end);
            out.push('\n');
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_main_content_and_strips_boilerplate() {
        let html = r##"
        <html>
            <head><title>Rust Ownership Guide</title></head>
            <body>
                <nav><a href="/home">Home</a><a href="/docs">Docs</a></nav>
                <main>
                    <h1>Ownership <a class="header" href="#ownership">§</a></h1>
                    <p>Rust uses <strong>ownership</strong> rules to manage <em>memory</em> safely.</p>
                    <aside>Sidebar promo</aside>
                    <pre><button>Copy</button><code class="language-rust">fn main() {
    let s = String::from("hi");
}</code></pre>
                </main>
                <footer>Copyright 2026</footer>
                <script>console.log("tracking")</script>
            </body>
        </html>
        "##;

        let page = extract_page(
            html,
            "https://doc.rust-lang.org/book/ch04.html",
            ExtractFormat::Markdown,
            None,
            0,
        )
        .unwrap();

        assert_eq!(page.title, "Rust Ownership Guide");
        assert!(!page.content.contains("Home"));
        assert!(!page.content.contains("Sidebar promo"));
        assert!(!page.content.contains("Copyright 2026"));
        assert!(!page.content.contains("tracking"));
        assert!(!page.content.contains("Copy"));
        assert!(page.content.contains("# Ownership"));
        assert!(!page.content.contains("§"));
        assert!(page.content.contains("**ownership**"));
        assert!(page.content.contains("*memory*"));
        assert!(page
            .content
            .contains("```rust\nfn main() {\n    let s = String::from(\"hi\");\n}\n```"));
    }

    #[test]
    fn renders_lists_tables_blockquotes_and_relative_links() {
        let html = r#"
        <html><body><article>
            <h1>Guide</h1>
            <p>Read <a href="/docs/intro">the intro</a> or <a href="sub/page.html">next</a>.</p>
            <ul>
                <li>First item</li>
                <li>Second item
                    <ol>
                        <li>Sub A</li>
                        <li>Sub B</li>
                    </ol>
                </li>
            </ul>
            <blockquote>Important warning note.</blockquote>
            <table>
                <thead><tr><th>Key</th><th>Value</th></tr></thead>
                <tbody><tr><td>A | B</td><td>100</td></tr></tbody>
            </table>
        </article></body></html>
        "#;

        let page = extract_page(
            html,
            "https://example.com/guide/start.html",
            ExtractFormat::Markdown,
            None,
            0,
        )
        .unwrap();

        assert!(page
            .content
            .contains("[the intro](https://example.com/docs/intro)"));
        assert!(page
            .content
            .contains("[next](https://example.com/guide/sub/page.html)"));
        assert!(page.content.contains("- First item\n- Second item\n  1. Sub A\n  2. Sub B"));
        assert!(page.content.contains("> Important warning note."));
        assert!(page.content.contains("| Key | Value |\n| --- | --- |\n| A \\| B | 100 |"));
    }

    #[test]
    fn shifts_headings_and_supports_custom_selector_and_text_mode() {
        let html = r#"
        <html>
            <head><title>API Docs</title></head>
            <body>
                <div class="ignored">Noise</div>
                <div class="target">
                    <h1>Section One</h1>
                    <p>Use <code>faro fetch</code> to read pages.</p>
                </div>
            </body>
        </html>
        "#;

        let md = extract_page(
            html,
            "https://example.com",
            ExtractFormat::Markdown,
            Some(".target"),
            2,
        )
        .unwrap();
        assert!(md.content.contains("### Section One"));
        assert!(md.content.contains("`faro fetch`"));
        assert!(!md.content.contains("Noise"));

        let txt = extract_page(
            html,
            "https://example.com",
            ExtractFormat::Text,
            Some(".target"),
            0,
        )
        .unwrap();
        assert!(txt.content.contains("Section One"));
        assert!(txt.content.contains("Use faro fetch to read pages."));
        assert!(!txt.content.contains('#'));
        assert!(!txt.content.contains('`'));
    }

    #[test]
    fn invalid_or_unmatched_selector_returns_error() {
        let html = "<html><body><p>hi</p></body></html>";
        assert!(extract_page(html, "https://x.test", ExtractFormat::Markdown, Some("[[bad"), 0).is_err());
        assert!(extract_page(html, "https://x.test", ExtractFormat::Markdown, Some(".missing"), 0).is_err());
    }
}
