/// Parsed command-line action for the LogAnalysis GUI executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliAction {
    PrintVersion,
    PrintHelp,
    UnknownFlag(String),
    OpenPaths(Vec<std::path::PathBuf>),
}

/// Returns the formatted CLI help text describing flags, `--`, and multi-file merge support.
pub fn help_text() -> String {
    format!(
        "LogAnalysis {}\n\
         GPU-Accelerated High Performance Log Analysis Tool\n\n\
         USAGE:\n\
             loganalysis [OPTIONS] [--] [FILE...]\n\n\
         ARGS:\n\
             [FILE...]         Open a single log file, or chronologically merge 2+ files\n\n\
         OPTIONS:\n\
             -h, --help        Print help information and exit\n\
             -v, -V, --version Print version information and exit\n\
             --                Treat all subsequent arguments as file paths",
        env!("CARGO_PKG_VERSION")
    )
}

/// Parses command-line arguments (excluding `argv[0]`) into a [`CliAction`].
pub fn parse_cli_args<I>(args: I) -> CliAction
where
    I: IntoIterator<Item = String>,
{
    let mut paths = Vec::new();
    let mut after_double_dash = false;

    for arg in args {
        if after_double_dash {
            paths.push(std::path::PathBuf::from(arg));
            continue;
        }
        if arg == "--" {
            after_double_dash = true;
            continue;
        }
        match arg.as_str() {
            "-h" | "--help" => return CliAction::PrintHelp,
            "--version" | "-V" | "-v" => return CliAction::PrintVersion,
            _ if arg.starts_with('-') => return CliAction::UnknownFlag(arg),
            _ => paths.push(std::path::PathBuf::from(arg)),
        }
    }

    CliAction::OpenPaths(paths)
}

pub fn handle_cli_version_flag<I>(args: I) -> bool
where
    I: IntoIterator<Item = String>,
{
    if matches!(parse_cli_args(args), CliAction::PrintVersion) {
        println!("LogAnalysis {}", env!("CARGO_PKG_VERSION"));
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_handle_cli_version_flag() {
        assert!(handle_cli_version_flag(vec!["--version".to_string()]));
        assert!(handle_cli_version_flag(vec!["-V".to_string()]));
        assert!(handle_cli_version_flag(vec!["-v".to_string()]));
        assert!(handle_cli_version_flag(vec!["foo.txt".to_string(), "--version".to_string()]));
        assert!(!handle_cli_version_flag(vec!["foo.txt".to_string()]));
        assert!(!handle_cli_version_flag(Vec::<String>::new()));
    }

    #[test]
    fn test_parse_cli_args_variants() {
        assert_eq!(parse_cli_args(Vec::<String>::new()), CliAction::OpenPaths(vec![]));
        assert_eq!(
            parse_cli_args(vec!["--help".into()]),
            CliAction::PrintHelp
        );
        assert_eq!(parse_cli_args(vec!["-h".into()]), CliAction::PrintHelp);
        assert_eq!(
            parse_cli_args(vec!["--version".into()]),
            CliAction::PrintVersion
        );
        assert_eq!(parse_cli_args(vec!["-V".into()]), CliAction::PrintVersion);
        assert_eq!(parse_cli_args(vec!["-v".into()]), CliAction::PrintVersion);
        assert_eq!(
            parse_cli_args(vec!["--bad".into()]),
            CliAction::UnknownFlag("--bad".into())
        );
        assert_eq!(
            parse_cli_args(vec!["--".into(), "-foo.log".into()]),
            CliAction::OpenPaths(vec![PathBuf::from("-foo.log")])
        );
        assert_eq!(
            parse_cli_args(vec!["a.log".into(), "b.log".into()]),
            CliAction::OpenPaths(vec![PathBuf::from("a.log"), PathBuf::from("b.log")])
        );
        assert!(help_text().contains("--help"));
        assert!(help_text().contains("--version"));
    }
}
