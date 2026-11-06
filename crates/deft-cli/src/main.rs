use std::io::{Read, Write};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let outcome = deft_cli::run(args, || {
        let mut buf = Vec::new();
        let _ = std::io::stdin().read_to_end(&mut buf);
        buf
    });
    let _ = std::io::stderr().write_all(&outcome.stderr);
    let _ = std::io::stdout().write_all(&outcome.stdout);
    let _ = std::io::stdout().flush();
    std::process::exit(outcome.code);
}
