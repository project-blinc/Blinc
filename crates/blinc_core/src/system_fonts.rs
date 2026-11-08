//! The proportional UI families a platform can be expected to have, in one place so the text
//! registry that resolves them and the themes that name them cannot drift apart.

/// Linux desktop UI families, best first: Adwaita Sans (GNOME 48 and newer), Cantarell (older
/// GNOME), Ubuntu, Noto Sans (KDE Plasma, Omarchy and most minimal installs) and DejaVu Sans.
///
/// A theme names the first as its family and the registry resolves the rest in order when
/// the one named is not installed.
pub const LINUX_SANS: &[&str] = &[
    "Adwaita Sans",
    "Cantarell",
    "Ubuntu",
    "Noto Sans",
    "DejaVu Sans",
];
