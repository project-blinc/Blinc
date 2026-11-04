use super::*;

pub(super) fn component(parent: &Component, example: &ChartExample) -> Component {
    let mut c = parent.clone();
    c.slug = example.slug.clone();
    c.title = example.title.clone();
    c.description = example.description.clone();
    c.entry = example.entry.clone();
    if let Some(capabilities) = &example.capabilities {
        c.capabilities = capabilities.clone();
    }
    c.examples.clear();
    c
}

pub(super) fn sections(root: &Path, parent: &Component, build: &str) -> Result<String> {
    let mut output = String::new();
    for example in &parent.examples {
        let c = component(parent, example);
        let mut html =
            include_str!("../../../../web/components/templates/chart-example.html").to_string();
        for (key, value) in [
            ("SLUG", pages::esc(&c.slug)),
            ("TITLE", pages::esc(&c.title)),
            ("DESCRIPTION", pages::esc(&c.description)),
            ("BUILD", build.into()),
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
            ("RUST", pages::esc(&pages::fixture_source(root, &c)?)),
            (
                "URL",
                format!(
                    "/components/cn/{}/examples/{}/?standalone=1&amp;embed=1&amp;tab=live",
                    parent.slug, c.slug
                ),
            ),
            (
                "OPEN_URL",
                format!(
                    "/components/cn/{}/examples/{}/?standalone=1&amp;tab=live",
                    parent.slug, c.slug
                ),
            ),
        ] {
            html = html.replace(&format!("{{{{{key}}}}}"), &value);
        }
        output.push_str(&html);
    }
    Ok(output)
}

pub(super) fn stage(
    root: &Path,
    dir: &Path,
    catalog: &Catalog,
    parent: &Component,
    revision: &str,
    build: &str,
    css: &str,
    routes: &mut Vec<String>,
) -> Result<()> {
    for example in &parent.examples {
        let c = component(parent, example);
        let route = format!("/components/cn/{}/examples/{}/", parent.slug, c.slug);
        let destination = dir
            .join("cn")
            .join(&parent.slug)
            .join("examples")
            .join(&c.slug);
        fs::create_dir_all(&destination)?;
        let html = pages::detail(root, catalog, &c, revision, build, css)?.replace(
            &format!("https://blinc.rs/components/cn/{}/", c.slug),
            &format!("https://blinc.rs/components/cn/{}/#{}", parent.slug, c.slug),
        );
        fs::write(destination.join("index.html"), html)?;
        routes.push(route);
    }
    if parent.slug == "charts" {
        let destination = dir.join("cn/line-chart");
        fs::create_dir_all(&destination)?;
        fs::write(
            destination.join("index.html"),
            "<!doctype html><html lang=\"en\"><meta charset=\"utf-8\"><meta http-equiv=\"refresh\" content=\"0;url=/components/cn/charts/#line-chart\"><link rel=\"canonical\" href=\"https://blinc.rs/components/cn/charts/\"><title>Charts · Blinc</title><a href=\"/components/cn/charts/#line-chart\">Charts</a></html>",
        )?;
        routes.push("/components/cn/line-chart/".into());
    }
    Ok(())
}
