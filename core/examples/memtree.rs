//! Prints the private working set of a process tree, the figure Oxigram
//! shows in its title: `cargo run --release -p oxigram-core --example memtree -- <pid>`.

fn main() {
    let Some(pid) = std::env::args().nth(1).and_then(|arg| arg.parse::<u32>().ok()) else {
        eprintln!("usage: memtree <pid>");
        std::process::exit(2);
    };
    match oxigram_core::mem::usage_of(pid) {
        Some(info) => println!(
            "{:.1} MB in {} processes",
            info.bytes as f64 / (1024.0 * 1024.0),
            info.processes
        ),
        None => {
            eprintln!("process {pid} not found");
            std::process::exit(1);
        }
    }
}
