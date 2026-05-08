use runtime::PersistentShell;

#[tokio::test]
async fn test_shell_basic_io() {
    let mut shell = PersistentShell::spawn().expect("Failed to spawn shell");
    let (out, err, code) = shell.execute("echo 'hello'", Some(1000)).await.unwrap();
    assert_eq!(out.trim(), "hello");
    assert!(err.is_empty());
    assert_eq!(code, Some(0));
}

#[tokio::test]
async fn test_shell_stateful_retention() {
    let mut shell = PersistentShell::spawn().expect("Failed to spawn shell");
    let (_, _, code1) = shell.execute("export KLAKO_VAR='state_retained'", Some(1000)).await.unwrap();
    assert_eq!(code1, Some(0));

    let (out, _, code2) = shell.execute("echo $KLAKO_VAR", Some(1000)).await.unwrap();
    assert_eq!(code2, Some(0));
    assert_eq!(out.trim(), "state_retained");
}

#[tokio::test]
async fn test_shell_stderr_and_errors() {
    let mut shell = PersistentShell::spawn().expect("Failed to spawn shell");
    let (out, err, code) = shell.execute("echo 'out'; echo 'err' >&2; exit 42", Some(1000)).await.unwrap();
    assert_eq!(out.trim(), "out");
    assert_eq!(err.trim(), "err");
    assert_eq!(code, Some(42));
}

#[tokio::test]
async fn test_shell_large_output() {
    let mut shell = PersistentShell::spawn().expect("Failed to spawn shell");
    // Generate ~65k of output to ensure we don't deadlock reading stdout and stderr
    let (out, err, code) = shell.execute("for i in $(seq 1 10000); do echo -n 'A'; done", Some(5000)).await.unwrap();
    assert_eq!(code, Some(0));
    assert!(err.is_empty());
    assert_eq!(out.trim().len(), 10000);
    assert!(out.starts_with("AAA"));
}

#[tokio::test]
async fn test_shell_multibyte_utf8_split_safety() {
    let mut shell = PersistentShell::spawn().expect("Failed to spawn shell");
    // Print a multi-byte character sequence that slices exactly across the default 1024-byte read chunk.
    // This forcibly splits a 4-byte emoji (🔥) across buffer reads.
    let cmd = "printf '%1023s' 'X' | tr ' ' '_'; echo '🔥💧🌳'";
    let (out, err, code) = shell.execute(cmd, Some(1000)).await.unwrap();
    assert_eq!(code, Some(0));
    assert!(err.is_empty(), "err was: {}", err);
    assert!(out.trim().ends_with("🔥💧🌳"), "Failed to decode multibyte chars properly! Output ended with: {:?}", out.trim().chars().rev().take(10).collect::<String>());
}
