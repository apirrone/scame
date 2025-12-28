use crossterm::style::Color;

/// Token types that can be highlighted
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenType {
    Keyword,
    Function,
    Type,
    String,
    Number,
    Comment,
    Operator,
    Variable,
    Constant,
    Parameter,
    Property,
    Punctuation,
    Error,
}

/// A color theme for syntax highlighting
#[derive(Debug, Clone)]
pub struct Theme {
    pub keyword: Color,
    pub function: Color,
    pub type_: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub operator: Color,
    pub variable: Color,
    pub constant: Color,
    pub parameter: Color,
    pub property: Color,
    pub punctuation: Color,
    pub error: Color,
    pub default: Color,
}

impl Theme {
    /// Create a default dark theme (similar to VS Code Dark+)
    pub fn default_dark() -> Self {
        Self {
            keyword: Color::Rgb { r: 197, g: 134, b: 192 },      // Purple
            function: Color::Rgb { r: 220, g: 220, b: 170 },     // Yellow
            type_: Color::Rgb { r: 78, g: 201, b: 176 },         // Teal
            string: Color::Rgb { r: 206, g: 145, b: 120 },       // Orange
            number: Color::Rgb { r: 181, g: 206, b: 168 },       // Light green
            comment: Color::Rgb { r: 106, g: 153, b: 85 },       // Green
            operator: Color::Rgb { r: 212, g: 212, b: 212 },     // Light gray
            variable: Color::Rgb { r: 156, g: 220, b: 254 },     // Light blue
            constant: Color::Rgb { r: 79, g: 193, b: 255 },      // Bright blue
            parameter: Color::Rgb { r: 156, g: 220, b: 254 },    // Light blue
            property: Color::Rgb { r: 156, g: 220, b: 254 },     // Light blue
            punctuation: Color::Rgb { r: 212, g: 212, b: 212 },  // Light gray
            error: Color::Rgb { r: 244, g: 71, b: 71 },          // Red
            default: Color::Rgb { r: 212, g: 212, b: 212 },      // Light gray
        }
    }

    /// Get color for a token type
    pub fn color_for(&self, token_type: TokenType) -> Color {
        match token_type {
            TokenType::Keyword => self.keyword,
            TokenType::Function => self.function,
            TokenType::Type => self.type_,
            TokenType::String => self.string,
            TokenType::Number => self.number,
            TokenType::Comment => self.comment,
            TokenType::Operator => self.operator,
            TokenType::Variable => self.variable,
            TokenType::Constant => self.constant,
            TokenType::Parameter => self.parameter,
            TokenType::Property => self.property,
            TokenType::Punctuation => self.punctuation,
            TokenType::Error => self.error,
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_dark()
    }
}
