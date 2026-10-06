//! Lexical analysis for HCL.
pub mod lexer;
pub mod token;
pub mod unescape;
pub use lexer::Lexer;
pub use token::{Token, TokenKind};
pub use unescape::unescape_string;
