use std::process::{Command, Stdio};
use std::io::{Read, Write};

fn main() {
    println!("Spawning sh...");
    let mut child = Command::new("sh")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    std::thread::spawn(move || {
        println!("Writing payload to sh...");
        let payload = "{ echo 'hello' ; } \n_ST=$?\necho \"__DELIM:$_ST\"\n";
        stdin.write_all(payload.as_bytes()).unwrap();
        stdin.flush().unwrap();
        println!("Payload written.");
    });

    let mut buf = vec![0; 1024];
    loop {
        println!("Waiting to read from stdout...");
        let n = stdout.read(&mut buf).unwrap();
        if n == 0 { break; }
        println!("READ: {:?}", String::from_utf8_lossy(&buf[..n]));
        if String::from_utf8_lossy(&buf[..n]).contains("__DELIM") {
            break;
        }
    }
}
