mod const_parse;
pub use const_parse::parse_or;

#[cfg(feature = "splice")]
mod splice;
#[cfg(feature = "splice")]
pub use splice::splice;
