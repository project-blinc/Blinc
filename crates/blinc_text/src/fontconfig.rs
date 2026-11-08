//! Opt-in use of fontconfig for the Linux UI face.
//!
//! By default the system UI face comes from a fixed list of the families Linux desktops ship
//! (`blinc_core::system_fonts::LINUX_SANS`), which costs nothing at startup. An app that wants
//! fontconfig's default sans instead calls [`use_fontconfig_sans`] once, before it draws text.
//! That runs the `fc-match` program, so it adds a process spawn to startup. fontconfig's default
//! sans is the distribution's choice, which is not always the desktop's own UI font.

/// The first family fontconfig matches for `pattern` (`sans-serif`, `monospace`, a family
/// name), by running `fc-match`. `None` when it is not installed or fails, and on platforms
/// other than Linux.
pub fn match_family(pattern: &str) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let output = std::process::Command::new("fc-match")
            .args([pattern, "-f", "%{family[0]}"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        first_family(&String::from_utf8_lossy(&output.stdout))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = pattern;
        None
    }
}

/// The first family of a comma-separated fontconfig family list.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn first_family(output: &str) -> Option<String> {
    let family = output.split(',').next()?.trim();
    (!family.is_empty()).then(|| family.to_string())
}

/// Resolve the system UI face through fontconfig's default sans instead of the built-in list.
/// Returns the family it chose, or `None` and leaves the built-in list in place when fontconfig
/// cannot be asked (not Linux, or no `fc-match`).
///
/// Call it once at startup, before the first text is drawn: it spawns a process, and glyphs
/// already cached for a family do not change under it. A theme that names a family of its own
/// still gets that family when it is installed; this changes what `system-ui` and a missing
/// family resolve to.
pub fn use_fontconfig_sans() -> Option<String> {
    let family = match_family("sans-serif")?;
    crate::global_font_registry()
        .lock()
        .ok()?
        .set_system_ui_family(Some(family.clone()));
    Some(family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_family_of_a_list_is_taken() {
        assert_eq!(first_family("Noto Sans"), Some("Noto Sans".to_string()));
        assert_eq!(
            first_family(" Noto Sans , Noto Sans CJK"),
            Some("Noto Sans".to_string())
        );
        assert_eq!(first_family(""), None);
        assert_eq!(first_family(" , A"), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn fontconfig_answers_when_it_is_installed() {
        if std::process::Command::new("fc-match")
            .arg("--version")
            .output()
            .is_err()
        {
            return;
        }
        let family = match_family("sans-serif").expect("fc-match answers");
        assert!(!family.is_empty());
    }
}
