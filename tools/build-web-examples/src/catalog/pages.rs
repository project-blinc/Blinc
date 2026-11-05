use super::*;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, html};

pub(super) fn esc(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
fn template(source: &str, values: &[(&str, String)]) -> String {
    values
        .iter()
        .fold(source.to_string(), |page, (key, value)| {
            page.replace(&format!("{{{{{key}}}}}"), value)
        })
}
fn sidebar(catalog: &Catalog, selected: &str) -> String {
    let mut result = format!(
        "<h2>Get started</h2><a href=\"/components/\">All components</a><a href=\"/components/installation/\" {}>Installation</a><h2>Components</h2>",
        if selected == "installation" {
            "aria-current=\"page\""
        } else {
            ""
        },
    );
    for c in &catalog.components {
        result.push_str(&format!(
            "<a href=\"/components/{}/{}/\" {}>{}</a>",
            esc(&c.library),
            esc(&c.slug),
            if selected == c.slug {
                "aria-current=\"page\""
            } else {
                ""
            },
            esc(&c.title)
        ));
    }
    result.push_str("<h2>Blocks</h2><small>Larger compositions, coming next.</small>");
    result
}
fn layout(
    catalog: &Catalog,
    selected: &str,
    title: &str,
    description: &str,
    body: String,
    script: &str,
    build: &str,
) -> String {
    template(
        include_str!("../../../../web/components/templates/layout.html"),
        &[
            ("TITLE", esc(title)),
            ("DESCRIPTION", esc(description)),
            ("VERSION", esc(&catalog.api_version)),
            ("SIDEBAR", sidebar(catalog, selected)),
            ("BODY", body),
            ("SCRIPT", script.into()),
            ("BUILD", build.into()),
            (
                "CANONICAL",
                if selected.is_empty() {
                    "https://blinc.rs/components/".into()
                } else if selected == "installation" {
                    "https://blinc.rs/components/installation/".into()
                } else {
                    format!(
                        "https://blinc.rs/components/{}/{selected}/",
                        catalog
                            .components
                            .iter()
                            .find(|c| c.slug == selected
                                || c.examples.iter().any(|e| e.slug == selected))
                            .map(|c| c.library.as_str())
                            .unwrap_or("cn")
                    )
                },
            ),
        ],
    )
}
pub(super) fn index(catalog: &Catalog, build: &str) -> String {
    let mut cards = String::new();
    let mut categories = std::collections::BTreeMap::<&str, usize>::new();
    for c in &catalog.components {
        *categories.entry(&c.category).or_default() += 1;
        let animation = if c.animated_preview() {
            "preview.gif"
        } else {
            "poster.png"
        };
        cards.push_str(&format!(r#"<a class="component-card" data-component="{}" data-category="{}" data-search="{}" href="/components/{}/{}/"><div class="card-preview"><img data-grid-preview src="/components/previews/{}/poster.png" data-poster="/components/previews/{}/poster.png" data-animation="/components/previews/{}/{animation}" width="{}" height="{}" alt="{} preview" loading="lazy"></div><div class="card-text"><div class="card-heading"><h2>{}</h2><span class="card-arrow" aria-hidden="true">↗</span></div><p>{}</p><div class="card-footer"><span class="badge">{}</span><span>View component</span></div></div></a>"#,
            esc(&c.slug), esc(&c.category), esc(&format!("{} {} {} {}", c.title, c.category, c.description, c.tags.join(" ")).to_lowercase()), esc(&c.library), esc(&c.slug), esc(&c.slug), esc(&c.slug), esc(&c.slug), c.viewport.width, c.viewport.height, esc(&c.title), esc(&c.title), esc(&c.description), esc(&c.category)));
    }
    let cards = cards
        .replace("/poster.png\"", &format!("/poster.png?v={build}\""))
        .replace("/preview.gif\"", &format!("/preview.gif?v={build}\""));
    let mut filters = format!(
        r#"<button class="filter-chip" type="button" data-filter="" aria-pressed="true">All<span>{}</span></button>"#,
        catalog.components.len()
    );
    for (category, count) in categories {
        filters.push_str(&format!(r#"<button class="filter-chip" type="button" data-filter="{}" aria-pressed="false">{}<span>{count}</span></button>"#, esc(category), esc(category)));
    }
    let body = template(
        include_str!("../../../../web/components/templates/index.html"),
        &[
            ("VERSION", esc(&catalog.api_version)),
            ("COUNT", catalog.components.len().to_string()),
            ("FILTERS", filters),
            ("CARDS", cards),
        ],
    );
    layout(
        catalog,
        "",
        "Components",
        "Blinc component demos, documentation and styling guides.",
        body,
        &format!("/components/releases/{build}/catalog.js"),
        build,
    )
}

pub(super) fn installation(root: &Path, catalog: &Catalog, build: &str) -> Result<String> {
    let source =
        fs::read_to_string(root.join("examples/blinc_component_demos/examples/installation.rs"))?;
    let body = template(
        include_str!("../../../../web/components/templates/installation.html"),
        &[("INSTALL_SOURCE", esc(&source))],
    );
    Ok(layout(
        catalog,
        "installation",
        "Installation",
        "Install Blinc and build your first reactive Rust interface.",
        body,
        &format!("/components/releases/{build}/code.js"),
        build,
    ))
}

pub(super) fn fixture_source(root: &Path, c: &Component) -> Result<String> {
    let source = fs::read_to_string(root.join(&c.source))?;
    let common = fs::read_to_string(root.join("examples/blinc_component_demos/src/demos.rs"))?;
    let shared = common
        .split("pub fn build(")
        .next()
        .ok_or("fixture setup missing")?;
    let rust = format!(
        "{shared}\n{}\n\n{}",
        if matches!(c.entry.as_str(), "notch_demo" | "canvas_demo") {
            source
                .replace("use crate::demos::DemoState;", "")
                .replace("//!", "//")
        } else {
            function(&source, &c.entry)?
        },
        function(&common, "bound_text")?
    );
    syn::parse_file(&rust).map_err(|error| format!("{} fixture source: {error}", c.slug))?;
    Ok(rust)
}

pub(super) fn detail(
    root: &Path,
    catalog: &Catalog,
    c: &Component,
    revision: &str,
    build: &str,
    css: &str,
) -> Result<String> {
    let rust = fixture_source(root, c)?;
    let props = c
        .props
        .iter()
        .map(|p| {
            format!(
                "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{}</td><td>{}</td></tr>",
                esc(&p.name),
                esc(&p.r#type),
                esc(&p.kind),
                esc(&p.example)
            )
        })
        .collect::<String>();
    let classes = c
        .classes
        .iter()
        .map(|s| format!("<li><code>{}</code></li>", esc(s)))
        .collect::<String>();
    let doc_url = c
        .doc
        .strip_prefix("docs/book/src/")
        .ok_or("doc path outside book")?
        .replace(".md", ".html");
    let docs = docs(root, c)?;
    let actual_css = component_css(css, &c.classes);
    let config = serde_json::json!({"customCss":c.custom_css.unwrap_or(true),"slug":c.slug,"title":c.title,"width":c.viewport.width,"height":c.viewport.height,"build":build,"animated":c.animated_preview(),"live":c.capabilities.iter().any(|v| v == "worker-canvas")});
    let body = template(
        include_str!("../../../../web/components/templates/detail.html"),
        &[
            ("SLUG", esc(&c.slug)),
            (
                "CSS_EDITOR_HIDDEN",
                if c.custom_css == Some(false) {
                    "hidden".into()
                } else {
                    String::new()
                },
            ),
            ("WIDTH", c.viewport.width.to_string()),
            (
                "OPEN_TAB",
                if c.capabilities.iter().any(|v| v == "worker-canvas") {
                    "live"
                } else {
                    "preview"
                }
                .into(),
            ),
            (
                "PLATFORM_SUPPORT",
                if c.capabilities.iter().any(|v| v == "worker-canvas") {
                    "The Live example runs in a dedicated worker and requires a compatible GPU surface. Preview and Code remain available when Live cannot start. The canvas has a descriptive label; a full screen-reader DOM mirror is not provided.".into()
                } else {
                    "This native media example needs the blinc_layout media feature. Live playback is not available in the gallery host. Preview, documentation, source and sample assets remain available.".into()
                },
            ),
            ("HEIGHT", c.viewport.height.to_string()),
            (
                "SETUP",
                if c.library == "cn" {
                    "Use the <a href=\"/components/installation/\">installation guide</a> to add <code>blinc_cn</code> and its component stylesheet.".into()
                } else {
                    "Use the <a href=\"/components/installation/\">installation guide</a> to create a Blinc application. This widget is available from <code>blinc_layout</code>; the demo uses the same Hybrid theme as the component library.".into()
                },
            ),
            (
                "ANIMATION",
                if c.animated_preview() {
                    "preview.gif"
                } else {
                    "poster.png"
                }
                .into(),
            ),
            (
                "ANIMATION_HIDDEN",
                if c.animated_preview() { "" } else { "hidden" }.into(),
            ),
            ("TITLE", esc(&c.title)),
            ("DESCRIPTION", esc(&c.description)),
            ("CATEGORY", esc(&c.category)),
            ("VERSION", esc(&catalog.api_version)),
            ("MATURITY", esc(&c.maturity)),
            (
                "EDITOR_PLACEHOLDER",
                esc(&fs::read_to_string(
                    root.join("web/components/recipes")
                        .join(c.recipe.clone().unwrap_or_else(|| {
                            if c.classes.iter().any(|class| class == ".cn-chart") {
                                "charts.css".to_string()
                            } else {
                                format!("{}.css", c.slug)
                            }
                        })),
                )?),
            ),
            ("RUST", esc(&rust)),
            ("EXTRA_EXAMPLES", super::charts::sections(root, c, build)?),
            (
                "PRIMARY_HEADING",
                if c.examples.is_empty() {
                    String::new()
                } else {
                    "<div id=\"line-chart\" class=\"chart-heading\"><h2>Line</h2><p>Compare two series with a smooth curve and value tooltips.</p></div>".into()
                },
            ),
            (
                "DEMO_CSS",
                esc(&fs::read_to_string(
                    root.join("examples/blinc_component_demos/demo.css"),
                )?),
            ),
            (
                "THEME_SOURCE",
                esc(&function(
                    &fs::read_to_string(root.join("examples/blinc_component_demos/src/lib.rs"))?,
                    "theme_bundle_for",
                )?),
            ),
            ("DEFAULT_CSS", esc(&actual_css)),
            ("PROPS", props),
            ("CLASSES", classes),
            ("KEYBOARD", esc(&c.keyboard)),
            ("NOTES", esc(&c.notes)),
            ("TOKENS", esc(&c.tokens)),
            ("DOCS", docs),
            ("DOC_URL", esc(&doc_url)),
            ("CONFIG", config.to_string().replace('<', "\\u003c")),
            (
                "SOURCE",
                format!("https://github.com/project-blinc/Blinc/tree/{revision}"),
            ),
            ("REVISION", esc(&revision[..12.min(revision.len())])),
            ("BUILD", build.into()),
            (
                "PROGRESS_HIDDEN",
                if c.slug == "progress" {
                    "".into()
                } else {
                    "hidden".into()
                },
            ),
            (
                "DISABLED_HIDDEN",
                if !c.controls.iter().any(|value| value == "disabled") {
                    "hidden".into()
                } else {
                    "".into()
                },
            ),
        ],
    );
    Ok(layout(
        catalog,
        &c.slug,
        &c.title,
        &c.description,
        body,
        &format!("/components/releases/{build}/detail.js"),
        build,
    ))
}
pub(super) fn function(source: &str, name: &str) -> Result<String> {
    let start = source
        .find(&format!("fn {name}("))
        .ok_or_else(|| format!("missing function {name}"))?;
    let start = if source[..start].ends_with("pub ") {
        start - 4
    } else {
        start
    };
    let mut depth = 0;
    let mut opened = false;
    for (offset, ch) in source[start..].char_indices() {
        if ch == '{' {
            depth += 1;
            opened = true;
        }
        if ch == '}' {
            depth -= 1;
            if depth == 0 && opened {
                return Ok(source[start..=start + offset].into());
            }
        }
    }
    Err(format!("unterminated function {name}").into())
}
fn docs(root: &Path, c: &Component) -> Result<String> {
    let markdown = fs::read_to_string(root.join(&c.doc))?;
    let markdown = if let Some(section) = &c.doc_section {
        markdown_section(&markdown, section)?
    } else {
        markdown.lines().skip(1).collect::<Vec<_>>().join("\n")
    };
    let parent = Path::new(c.doc.strip_prefix("docs/book/src/").unwrap())
        .parent()
        .unwrap();
    let parser = Parser::new_ext(
        &markdown,
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH,
    )
    .map(|event| {
        if let Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) = event
        {
            let url = if !dest_url.contains("://")
                && !dest_url.starts_with('#')
                && !dest_url.starts_with('/')
            {
                format!(
                    "/docs/{}",
                    parent
                        .join(dest_url.as_ref())
                        .to_string_lossy()
                        .replace(".md", ".html")
                )
            } else {
                dest_url.to_string()
            };
            Event::Start(Tag::Link {
                link_type,
                dest_url: CowStr::from(url),
                title,
                id,
            })
        } else {
            event
        }
    });
    let mut output = String::new();
    html::push_html(&mut output, parser);
    Ok(output)
}
fn markdown_section(markdown: &str, section: &str) -> Result<String> {
    use pulldown_cmark::TagEnd;
    let mut heading = None;
    let mut title = String::new();
    let mut start = None;
    let mut end = markdown.len();
    for (event, range) in Parser::new(markdown).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                if let Some((_, selected_level)) = start {
                    if level <= selected_level {
                        end = range.start;
                        break;
                    }
                }
                heading = Some(level);
                title.clear();
            }
            Event::Text(text) | Event::Code(text) if heading.is_some() => title.push_str(&text),
            Event::End(TagEnd::Heading(_)) => {
                if title == section {
                    start = Some((range.end, heading.unwrap()));
                }
                heading = None;
            }
            _ => {}
        }
    }
    let (start, _) = start.ok_or_else(|| format!("missing book section: {section}"))?;
    Ok(markdown[start..end].to_string())
}

fn component_css(css: &str, classes: &[String]) -> String {
    let css = css.strip_prefix("@layer components {\n").unwrap_or(css);
    let mut result = String::new();
    let mut start = 0;
    let mut depth = 0;
    for (index, ch) in css.char_indices() {
        if ch == '{' {
            depth += 1;
        }
        if ch == '}' {
            depth -= 1;
            if depth == 0 {
                let block = &css[start..=index];
                let header = block.split('{').next().unwrap_or("");
                if classes.iter().any(|class| header.contains(class)) {
                    result.push_str(block.trim());
                    result.push_str("\n");
                }
                start = index + 1;
            }
        }
    }
    result
}
pub(super) fn sitemap(catalog: &Catalog) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\"><url><loc>https://blinc.rs/components/</loc></url><url><loc>https://blinc.rs/components/installation/</loc></url>",
    );
    for c in &catalog.components {
        xml.push_str(&format!(
            "<url><loc>https://blinc.rs/components/{}/{}/</loc></url>",
            c.library, c.slug
        ));
    }
    xml.push_str("</urlset>");
    xml
}

#[cfg(test)]
mod tests {
    use super::*;
    fn catalog() -> Catalog {
        serde_json::from_str(include_str!("../../../../web/components/catalog.json")).unwrap()
    }
    #[test]
    fn catalog_pages_have_one_selected_demo_and_exact_fixture_source() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let catalog = catalog();
        super::super::validate(root, &catalog).unwrap();
        for component in &catalog.components {
            let page = detail(
                root,
                &catalog,
                component,
                "0123456789abcdef",
                "build-test",
                &super::super::default_css(root).unwrap(),
            )
            .unwrap();
            assert_eq!(page.matches("id=\"live-canvas\"").count(), 1);
            assert!(page.contains(&format!("\"slug\":\"{}\"", component.slug)));
            assert!(page.contains("Preview</button>"));
            assert!(page.contains("Live</button>"));
            assert!(page.contains("Code</button>"));
            assert!(page.contains("id=\"custom-css\""));
            assert!(page.contains("role=\"switch\" id=\"demo-dark\""));
            assert_eq!(page.matches("data-theme-family=").count(), 3);
            assert!(page.contains("/components/installation/"));
            assert!(!page.contains("{{"), "unexpanded page template");
            let config_start = page.find("id=\"component-config\">").unwrap();
            let config = &page[config_start..];
            for other in &catalog.components {
                if other.slug != component.slug {
                    assert!(!config.contains(&format!("\"slug\":\"{}\"", other.slug)));
                }
            }
        }
        let index = index(&catalog, "test");
        assert!(!index.contains("detail.js"));
        assert!(!index.contains(".wasm"));
        assert!(!index.contains("<select"));
        assert_eq!(
            index.matches("data-grid-preview").count(),
            catalog.components.len()
        );
        assert_eq!(
            index.matches("data-component=").count(),
            catalog.components.len()
        );
        let installation = installation(root, &catalog, "test").unwrap();
        assert!(installation.contains("WindowedApp::run_with_theme"));
        assert!(installation.contains("language-rust"));
        assert!(!installation.contains("http-equiv=\"refresh\""));
        assert!(!installation.contains("{{"));
    }
    #[test]
    fn invalid_catalog_routes_are_rejected() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let mut catalog = catalog();
        catalog.components[0].slug = "../button".into();
        assert!(super::super::validate(root, &catalog).is_err());
    }
}
