pub const fn parse_or(s: Option<&str>, default: usize) -> usize {
    const fn rec(x: usize, s: &[u8]) -> usize {
        if let Some((&c, s)) = s.split_first() {
            match c {
                b'0'..=b'9' => rec(10 * x + (c as usize - b'0' as usize), s),
                _ => panic!("Error parsing constants"),
            }
        } else {
            x
        }
    }

    match s {
        Some(s) => rec(0, s.as_bytes()),
        None => default,
    }
}
