fn main() {
    println!("[WindowExample] start");

    let ui = viewkit::card!()
        .children([
            viewkit::text!("Hello from ViewKit").into_elem(),
            viewkit::text!("(no Kagami IPC in app code)").into_elem(),
        ])
        .into_elem();

    viewkit::App::new(ui)
        .title("WindowExample")
        .run();
}
