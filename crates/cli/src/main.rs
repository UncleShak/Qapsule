use qapsule_protocol::PROTOCOL_VERSION;

fn version_line() -> String {
    format!(
        "qapsule {} (protocol v{PROTOCOL_VERSION})",
        env!("CARGO_PKG_VERSION")
    )
}

fn main() {
    println!("{}", version_line());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_line_names_the_protocol() {
        assert!(version_line().ends_with("(protocol v1)"));
    }
}
