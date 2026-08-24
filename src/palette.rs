//! Tokyo Night Storm truecolor values used for terminal output.

pub const COMMENT: (u8, u8, u8) = (86, 95, 137);
pub const CYAN: (u8, u8, u8) = (125, 207, 255);
pub const TEAL: (u8, u8, u8) = (115, 218, 202);
pub const GREEN: (u8, u8, u8) = (158, 206, 106);
pub const ORANGE: (u8, u8, u8) = (224, 175, 104);
pub const RED: (u8, u8, u8) = (247, 118, 142);

pub trait Paint {
    fn tc(&self, rgb: (u8, u8, u8)) -> colored::ColoredString;
}

impl Paint for str {
    fn tc(&self, rgb: (u8, u8, u8)) -> colored::ColoredString {
        use colored::Colorize;
        self.truecolor(rgb.0, rgb.1, rgb.2)
    }
}

impl Paint for String {
    fn tc(&self, rgb: (u8, u8, u8)) -> colored::ColoredString {
        self.as_str().tc(rgb)
    }
}
