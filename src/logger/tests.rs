use super::*;
use std::sync::Mutex;

// Serialise tests because logger uses global state
static TEST_MUTEX: Mutex<()> = Mutex::new(());

#[test]
fn test_init_disabled() {
    let _g = TEST_MUTEX.lock().unwrap();
    temp_env::with_var("RDS_LOG", None::<&str>, || {
        init(false);
        assert!(!enabled());
        assert_eq!(level(), None);
    });
    assert!(!enabled());
    assert_eq!(level(), None);
}

#[test]
fn test_init_enabled_info_by_default() {
    let _g = TEST_MUTEX.lock().unwrap();
    temp_env::with_var("RDS_LOG", None::<&str>, || {
        init(true);
        assert!(enabled());
        assert_eq!(level(), Some(Level::Info));
        // reset
        init(false);
    });
}

#[test]
fn test_init_enabled_debug_from_env() {
    let _g = TEST_MUTEX.lock().unwrap();
    temp_env::with_var("RDS_LOG", Some("debug"), || {
        init(true);
        assert!(enabled());
        assert_eq!(level(), Some(Level::Debug));
    });
    temp_env::with_var("RDS_LOG", None::<&str>, || {
        init(false);
    });
}

#[test]
fn test_logging_output_format_info() {
    let _g = TEST_MUTEX.lock().unwrap();
    // enable logger at info level
    unsafe {
        std::env::remove_var("RDS_LOG");
    }
    init(true);
    info("hello world");
    // test-only accessor
    let last = crate::logger::test_last_log();
    assert!(last.is_some());
    let s = last.unwrap();
    assert!(s.contains("[rds::INFO]"));
    assert!(s.contains("hello world"));
    init(false);
}

#[test]
fn test_logging_output_format_debug() {
    let _g = TEST_MUTEX.lock().unwrap();
    temp_env::with_var("RDS_LOG", Some("debug"), || {
        init(true);
        debug("dbg-msg");
        let last = crate::logger::test_last_log();
        assert!(last.is_some());
        let s = last.unwrap();
        assert!(s.contains("[rds::DEBUG]"));
        assert!(s.contains("dbg-msg"));
    });
    temp_env::with_var("RDS_LOG", None::<&str>, || {
        init(false);
    });
}
