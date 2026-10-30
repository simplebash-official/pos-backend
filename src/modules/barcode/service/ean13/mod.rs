// Pure EAN-13 encoding — no I/O, no DB. Uses the GS1-reserved `20`-`29`
// prefix range (standard practice for in-store/internal-use barcodes, the
// same mechanism grocery stores use for scale/deli labels) so generated
// codes never collide with a real published GTIN.

mod checksum_digit;
mod encode;
#[cfg(test)]
mod tests;

#[cfg(test)]
use checksum_digit::checksum_digit;
pub(crate) use encode::encode;
