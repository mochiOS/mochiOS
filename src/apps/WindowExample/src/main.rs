use viewkit::*;

fn main() {
    println!("[WindowExample] start");

    let ui = card!()
        .children([
            text!("Hello from ViewKit").into_elem(),
        ])
        .into_elem();

    App::new(ui)
        .title("WindowExample")
        .run();
}
