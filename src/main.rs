pub mod config;
pub mod game;
pub mod object;

use config::{LIMIT_FPS, SCREEN_HEIGHT, SCREEN_WIDTH};
use game::{main_menu, Tcod};

use tcod::console::*;

fn main() {
    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();

    let mut tcod = Tcod::new(root);

    main_menu(&mut tcod);
}
