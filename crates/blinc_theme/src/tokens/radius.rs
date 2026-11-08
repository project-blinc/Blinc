//! Border radius tokens for theming

/// Semantic radius token keys for dynamic access
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum RadiusToken {
    None,
    Sm,
    Default,
    Md,
    Lg,
    Xl,
    Xxl,
    Xxxl,
    Full,
}

/// Complete set of border radius tokens
#[derive(Clone, Debug)]
pub struct RadiusTokens {
    pub radius_none: f32,
    pub radius_sm: f32,
    pub radius_default: f32,
    pub radius_md: f32,
    pub radius_lg: f32,
    pub radius_xl: f32,
    pub radius_2xl: f32,
    pub radius_3xl: f32,
    pub radius_full: f32,
}

impl RadiusTokens {
    /// Get radius value by token key
    pub fn get(&self, token: RadiusToken) -> f32 {
        match token {
            RadiusToken::None => self.radius_none,
            RadiusToken::Sm => self.radius_sm,
            RadiusToken::Default => self.radius_default,
            RadiusToken::Md => self.radius_md,
            RadiusToken::Lg => self.radius_lg,
            RadiusToken::Xl => self.radius_xl,
            RadiusToken::Xxl => self.radius_2xl,
            RadiusToken::Xxxl => self.radius_3xl,
            RadiusToken::Full => self.radius_full,
        }
    }

    /// Replace the value for `token`; the theme's runtime overrides apply through this.
    pub(crate) fn set(&mut self, token: RadiusToken, value: f32) {
        match token {
            RadiusToken::None => self.radius_none = value,
            RadiusToken::Sm => self.radius_sm = value,
            RadiusToken::Default => self.radius_default = value,
            RadiusToken::Md => self.radius_md = value,
            RadiusToken::Lg => self.radius_lg = value,
            RadiusToken::Xl => self.radius_xl = value,
            RadiusToken::Xxl => self.radius_2xl = value,
            RadiusToken::Xxxl => self.radius_3xl = value,
            RadiusToken::Full => self.radius_full = value,
        }
    }
}

impl Default for RadiusTokens {
    fn default() -> Self {
        Self {
            radius_none: 0.0,
            radius_sm: 2.0,
            radius_default: 4.0,
            radius_md: 6.0,
            radius_lg: 8.0,
            radius_xl: 12.0,
            radius_2xl: 16.0,
            radius_3xl: 24.0,
            radius_full: 9999.0,
        }
    }
}
