//! `cargo xtask commands`: the map from command to source file and options.

use deft_ops::generated::SOURCES;

pub fn print() {
    println!("| Command | Also reachable as | Defined in | Options |\n|---|---|---|---|");
    for (dir, stem, op) in SOURCES {
        let also: Vec<&str> = op.mounts.to_vec();
        let options: Vec<String> = op.params.iter().map(|p| format!("--{}", p.name)).collect();
        println!(
            "| deft {} | {} | crates/deft-ops/src/ops/{dir}/{stem}.rs | {} |",
            op.path(),
            also.join(", "),
            options.join(" ")
        );
    }
}
