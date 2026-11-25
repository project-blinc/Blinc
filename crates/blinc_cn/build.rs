//! Compiles `styles/` into the `CN_STYLES` constant.

fn main() {
    blinc_sass::build("styles/index.scss", "cn.css", Some("components"));
}
