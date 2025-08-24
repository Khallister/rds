use super::*;

#[test]
fn test_cli_validation_conflicting_cache_flags() {
    let mut cli = Cli::parse_args();
    cli.cache = true;
    cli.no_cache = true;

    assert!(cli.validate().is_err());
}

#[test]
fn test_cli_validation_zero_threads() {
    let mut cli = Cli::parse_args();
    cli.threads = Some(0);

    assert!(cli.validate().is_err());
}

#[test]
fn test_effective_cache_setting() {
    let mut cli = Cli::parse_args();

    assert!(!cli.effective_cache_setting());

    cli.watch = true;
    assert!(cli.effective_cache_setting());

    cli.no_cache = true;
    assert!(!cli.effective_cache_setting());

    cli.cache = true;
    cli.no_cache = false;
    assert!(cli.effective_cache_setting());
}
