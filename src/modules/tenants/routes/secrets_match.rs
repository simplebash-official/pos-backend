/// Compares without an early exit on the first differing byte, so response time
/// doesn't reveal how much of a guessed secret was right.
pub(super) fn secrets_match(provided: &[u8], expected: &[u8]) -> bool {
    let mut diff = provided.len() ^ expected.len();
    for i in 0..provided.len().max(expected.len()) {
        let a = provided.get(i).copied().unwrap_or(0);
        let b = expected.get(i).copied().unwrap_or(0);
        diff |= usize::from(a ^ b);
    }
    diff == 0
}
