//! Spacing tokens for theming (4px base scale)

/// Semantic spacing token keys for dynamic access
#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum SpacingToken {
    Space0,
    Space0_5,
    Space1,
    Space1_5,
    Space2,
    Space2_5,
    Space3,
    Space3_5,
    Space4,
    Space5,
    Space6,
    Space7,
    Space8,
    Space9,
    Space10,
    Space11,
    Space12,
    Space14,
    Space16,
    Space20,
    Space24,
    Space28,
    Space32,
}

/// Complete set of spacing tokens (4px base scale)
#[derive(Clone, Debug)]
pub struct SpacingTokens {
    pub space_0: f32,
    pub space_0_5: f32,
    pub space_1: f32,
    pub space_1_5: f32,
    pub space_2: f32,
    pub space_2_5: f32,
    pub space_3: f32,
    pub space_3_5: f32,
    pub space_4: f32,
    pub space_5: f32,
    pub space_6: f32,
    pub space_7: f32,
    pub space_8: f32,
    pub space_9: f32,
    pub space_10: f32,
    pub space_11: f32,
    pub space_12: f32,
    pub space_14: f32,
    pub space_16: f32,
    pub space_20: f32,
    pub space_24: f32,
    pub space_28: f32,
    pub space_32: f32,
}

impl SpacingTokens {
    /// Get spacing value by token key
    pub fn get(&self, token: SpacingToken) -> f32 {
        match token {
            SpacingToken::Space0 => self.space_0,
            SpacingToken::Space0_5 => self.space_0_5,
            SpacingToken::Space1 => self.space_1,
            SpacingToken::Space1_5 => self.space_1_5,
            SpacingToken::Space2 => self.space_2,
            SpacingToken::Space2_5 => self.space_2_5,
            SpacingToken::Space3 => self.space_3,
            SpacingToken::Space3_5 => self.space_3_5,
            SpacingToken::Space4 => self.space_4,
            SpacingToken::Space5 => self.space_5,
            SpacingToken::Space6 => self.space_6,
            SpacingToken::Space7 => self.space_7,
            SpacingToken::Space8 => self.space_8,
            SpacingToken::Space9 => self.space_9,
            SpacingToken::Space10 => self.space_10,
            SpacingToken::Space11 => self.space_11,
            SpacingToken::Space12 => self.space_12,
            SpacingToken::Space14 => self.space_14,
            SpacingToken::Space16 => self.space_16,
            SpacingToken::Space20 => self.space_20,
            SpacingToken::Space24 => self.space_24,
            SpacingToken::Space28 => self.space_28,
            SpacingToken::Space32 => self.space_32,
        }
    }

    /// Replace the value for `token`; the theme's runtime overrides apply through this.
    pub(crate) fn set(&mut self, token: SpacingToken, value: f32) {
        match token {
            SpacingToken::Space0 => self.space_0 = value,
            SpacingToken::Space0_5 => self.space_0_5 = value,
            SpacingToken::Space1 => self.space_1 = value,
            SpacingToken::Space1_5 => self.space_1_5 = value,
            SpacingToken::Space2 => self.space_2 = value,
            SpacingToken::Space2_5 => self.space_2_5 = value,
            SpacingToken::Space3 => self.space_3 = value,
            SpacingToken::Space3_5 => self.space_3_5 = value,
            SpacingToken::Space4 => self.space_4 = value,
            SpacingToken::Space5 => self.space_5 = value,
            SpacingToken::Space6 => self.space_6 = value,
            SpacingToken::Space7 => self.space_7 = value,
            SpacingToken::Space8 => self.space_8 = value,
            SpacingToken::Space9 => self.space_9 = value,
            SpacingToken::Space10 => self.space_10 = value,
            SpacingToken::Space11 => self.space_11 = value,
            SpacingToken::Space12 => self.space_12 = value,
            SpacingToken::Space14 => self.space_14 = value,
            SpacingToken::Space16 => self.space_16 = value,
            SpacingToken::Space20 => self.space_20 = value,
            SpacingToken::Space24 => self.space_24 = value,
            SpacingToken::Space28 => self.space_28 = value,
            SpacingToken::Space32 => self.space_32 = value,
        }
    }

    /// Create spacing tokens with a custom base unit
    pub fn with_base(base: f32) -> Self {
        Self {
            space_0: 0.0,
            space_0_5: base * 0.5,
            space_1: base,
            space_1_5: base * 1.5,
            space_2: base * 2.0,
            space_2_5: base * 2.5,
            space_3: base * 3.0,
            space_3_5: base * 3.5,
            space_4: base * 4.0,
            space_5: base * 5.0,
            space_6: base * 6.0,
            space_7: base * 7.0,
            space_8: base * 8.0,
            space_9: base * 9.0,
            space_10: base * 10.0,
            space_11: base * 11.0,
            space_12: base * 12.0,
            space_14: base * 14.0,
            space_16: base * 16.0,
            space_20: base * 20.0,
            space_24: base * 24.0,
            space_28: base * 28.0,
            space_32: base * 32.0,
        }
    }
}

impl Default for SpacingTokens {
    fn default() -> Self {
        // 4px base scale (Tailwind-inspired)
        Self::with_base(4.0)
    }
}
