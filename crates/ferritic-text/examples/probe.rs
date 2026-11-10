use ferritic_core::{Point, TextStyle, TextEngine, Color};
use ferritic_text::SystemText;

fn main() {
    let mut text = SystemText::new().unwrap();
    text.set_scale(2.0);
    let style = TextStyle::new(15.0, Color::BLACK);
    println!("measure: {:?}", text.measure("Ada", &style));
    println!("line_height: {}", text.line_height(15.0));
    let mut quads = Vec::new();
    text.layout("Ada", &style, Point::new(0.0, 0.0), &mut quads);
    for q in &quads {
        println!("quad rect {:?} uv {:?}", q.rect, q.uv);
    }
}
