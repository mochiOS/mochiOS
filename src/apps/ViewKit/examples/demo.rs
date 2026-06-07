use viewkit::ui::UIElement;

viewkit::components! {
    card: from_str!("resources/components/card.html"),
    text: from_str!("resources/components/text.html"),
    image: from_str!("resources/components/image.html"),
}

fn main() {
    let ui: UIElement = card()
        .attr("padding", 20)
        .children([
            text().content_string("Hello from generated components").into_elem(),
            image().content_image("assets/logo.png").into_elem(),
        ])
        .into_elem();

    println!("{}", serde_json::to_string_pretty(&ui.into_json()).unwrap());
}
