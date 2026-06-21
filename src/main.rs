fn main() {
    if let Err(err) = davbox::cli::run(std::env::args().skip(1)) {
        eprintln!("{err}");
        std::process::exit(1);
    }
}
