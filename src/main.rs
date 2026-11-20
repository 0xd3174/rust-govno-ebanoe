fn main() {
    if let Err(error) = dear_imgui_demo::run_native() {
        eprintln!("fatal: {error}");
        std::process::exit(1);
    }
}
