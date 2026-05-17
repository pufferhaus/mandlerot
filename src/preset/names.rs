use std::time::{SystemTime, UNIX_EPOCH};

const TERMS: &[&str] = &[
    "ubik", "pkd", "gibson", "neuro", "tron", "vurt", "snowcrsh", "ono",
    "warp", "hyperspc", "lambda", "axion", "qubit", "tachyon", "vector",
    "manifold", "lattice",
    "replcnt", "decker", "wirehead", "nomad", "ronin", "chrome",
    "arasaka", "zaibatsu", "orbital", "void", "noosphr", "matrix",
    "glitch", "drift", "flux", "phase", "decay", "echo",
];

pub fn random_look_name() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0);
    let a = nanos % TERMS.len();
    let b = (nanos / 7 + 1) % TERMS.len();
    format!("{}-{}", TERMS[a], TERMS[b])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn term_pool_is_nonempty() {
        assert!(!TERMS.is_empty());
        for t in TERMS {
            assert!(t.len() <= 8, "term too long: {t}");
            assert!(!t.is_empty(), "empty term");
        }
    }

    #[test]
    fn random_name_uses_hyphen_separator() {
        let n = random_look_name();
        let parts: Vec<&str> = n.split('-').collect();
        assert_eq!(parts.len(), 2, "expected exactly one hyphen: {n}");
        assert!(TERMS.contains(&parts[0]));
        assert!(TERMS.contains(&parts[1]));
    }

    #[test]
    fn random_name_stays_under_17_chars() {
        for _ in 0..50 {
            let n = random_look_name();
            assert!(n.len() <= 17, "name too long: {n}");
        }
    }
}
