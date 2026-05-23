use viewkit::{components, App};

fn main() {
    println!("[TestClient] starting");

    println!("[TestClient] building ViewKit UI");
    let ui = components::card()
        .children([
            components::text().text("Hello from ViewKit components").into_elem(),
            components::card()
                .children([components::text().label("Nested card").into_elem()])
                .into_elem(),
        ])
        .into_elem();

    println!("[TestClient] running ViewKit app");
    App::new(ui).run();
}
