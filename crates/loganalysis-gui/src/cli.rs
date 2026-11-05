pub fn handle_cli_version_flag<I>(args: I) -> bool
where
    I: IntoIterator<Item = String>,
{
    for arg in args {
        if arg == "--version" || arg == "-V" || arg == "-v" {
            println!("LogAnalysis {}", env!("CARGO_PKG_VERSION"));
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_cli_version_flag() {
        assert!(handle_cli_version_flag(vec!["--version".to_string()]));
        assert!(handle_cli_version_flag(vec!["-V".to_string()]));
        assert!(handle_cli_version_flag(vec!["-v".to_string()]));
        assert!(handle_cli_version_flag(vec!["foo.txt".to_string(), "--version".to_string()]));
        assert!(!handle_cli_version_flag(vec!["foo.txt".to_string()]));
        assert!(!handle_cli_version_flag(Vec::<String>::new()));
    }
}
