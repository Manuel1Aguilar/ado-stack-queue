fn main() {
    let dir = std::env::var("DATA_DIR").expect("DATA_DIR is required");
    let port = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(0);
    let twin = twin_ado::Twin::start_on(std::path::Path::new(&dir), port);
    println!("twin-ado listening on {}", twin.addr);
    loop {
        std::thread::park();
    }
}
