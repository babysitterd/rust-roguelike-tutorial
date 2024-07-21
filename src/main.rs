use rand::Rng;
use std::cmp;

use tcod::colors::*;
use tcod::console::*;

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const LIMIT_FPS: i32 = 20;

// size of the map
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 45;

// parameters for dungeon generator
const ROOM_MAX_SIZE: i32 = 10;
const ROOM_MIN_SIZE: i32 = 6;
const MAX_ROOMS: i32 = 30;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_DARK_GROUND: Color = Color {
    r: 50,
    g: 50,
    b: 150,
};

struct Rectangle {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl Rectangle {
    fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rectangle {
            x1: x,
            y1: y,
            x2: x + w,
            y2: y + h,
        }
    }

    fn center(&self) -> (i32, i32) {
        ((self.x1 + self.x2) / 2, (self.y1 + self.y2) / 2)
    }

    fn intersects(&self, other: &Rectangle) -> bool {
        self.x1 <= other.x2 && self.x2 >= other.x1 && self.y1 <= other.y2 && self.y2 >= other.y1
    }
}

/// This is a generic object: the player, a monster, an item, the stairs...
/// It's always represented by a character on screen.
#[derive(Clone, Debug)]
struct Object {
    x: i32,
    y: i32,
    char: char,
    color: Color,
}

impl Object {
    pub fn new(x: i32, y: i32, char: char, color: Color) -> Self {
        Object { x, y, char, color }
    }

    /// move by the given amount
    pub fn move_by(&mut self, dx: i32, dy: i32) {
        self.x += dx;
        self.y += dy;
    }

    /// set the color and then draw the character that represents this object at its position
    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.char, BackgroundFlag::None);
    }
}

/// A tile of the map and its properties
#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    block_sight: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            block_sight: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            block_sight: true,
        }
    }
}

type Map = Vec<Vec<Tile>>;

struct Game {
    map: Map,
}

fn carve_room(room: &Rectangle, map: &mut Map) {
    for i in (room.x1 + 1)..room.x2 {
        for j in (room.y1 + 1)..room.y2 {
            map[i as usize][j as usize] = Tile::empty();
        }
    }
}

fn carve_h_tunnel(x1: i32, x2: i32, y: i32, map: &mut Map) {
    let from = cmp::min(x1, x2);
    let to = cmp::max(x1, x2) + 1;
    for x in from..to {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn carve_v_tunnel(y1: i32, y2: i32, x: i32, map: &mut Map) {
    let from = cmp::min(y1, y2);
    let to = cmp::max(y1, y2) + 1;
    for y in from..to {
        map[x as usize][y as usize] = Tile::empty();
    }
}

fn make_map(player: &mut Object) -> Map {
    let mut map = vec![vec![Tile::wall(); MAP_HEIGHT as usize]; MAP_WIDTH as usize];

    let mut rooms: Vec<Rectangle> = vec![];

    for _ in 0..MAX_ROOMS {
        // random width and height
        let w = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        let h = rand::thread_rng().gen_range(ROOM_MIN_SIZE, ROOM_MAX_SIZE + 1);
        // random position without going out of the boundaries of the map
        let x = rand::thread_rng().gen_range(0, MAP_WIDTH - w);
        let y = rand::thread_rng().gen_range(0, MAP_HEIGHT - h);

        let new_room = Rectangle::new(x, y, w, h);

        let failed = rooms.iter().any(|other| new_room.intersects(other));

        if !failed {
            let (new_x, new_y) = new_room.center();

            if let Some(prev) = rooms.last() {
                let (prev_x, prev_y) = prev.center();

                if rand::random() {
                    carve_h_tunnel(prev_x, new_x, prev_y, &mut map);
                    carve_v_tunnel(prev_y, new_y, new_x, &mut map);
                } else {
                    carve_v_tunnel(prev_y, new_y, prev_x, &mut map);
                    carve_h_tunnel(prev_x, new_x, new_y, &mut map);
                }
            } else {
                player.x = new_x;
                player.y = new_y;
            }

            rooms.push(Rectangle::new(x, y, w, h));
        }
    }

    for room in &rooms {
        carve_room(room, &mut map)
    }

    map
}

fn render_all(tcod: &mut Tcod, map: &Map, objects: &[Object]) {
    // render map
    for j in 0..MAP_HEIGHT {
        for i in 0..MAP_WIDTH {
            let wall = map[i as usize][j as usize].block_sight;
            if wall {
                tcod.con
                    .set_char_background(i, j, COLOR_DARK_WALL, BackgroundFlag::Set);
            } else {
                tcod.con
                    .set_char_background(i, j, COLOR_DARK_GROUND, BackgroundFlag::Set);
            }
        }
    }

    // render objects
    for obj in objects {
        obj.draw(&mut tcod.con);
    }

    // blit the contents of "con" to the root console and present it
    blit(
        &tcod.con,
        (0, 0),
        (MAP_WIDTH, MAP_HEIGHT),
        &mut tcod.root,
        (0, 0),
        1.0,
        1.0,
    );
}

struct Tcod {
    root: Root,
    con: Offscreen,
}

fn handle_keys(tcod: &mut Tcod, player: &mut Object, map: &Map) -> bool {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;

    let mut new_player = player.clone();
    let key = tcod.root.wait_for_keypress(true);
    match key {
        Key {
            code: Enter,
            alt: true,
            ..
        } => {
            // Alt+Enter: toggle fullscreen
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
        }
        Key { code: Escape, .. } => return true, // exit game

        // movement keys
        Key { code: Up, .. } => new_player.move_by(0, -1),
        Key { code: Down, .. } => new_player.move_by(0, 1),
        Key { code: Left, .. } => new_player.move_by(-1, 0),
        Key { code: Right, .. } => new_player.move_by(1, 0),

        _ => {}
    }

    // TODO: is this check better placed in the move_by function?
    if new_player.x >= 0
        && new_player.x < MAP_WIDTH
        && new_player.y >= 0
        && new_player.y < MAP_HEIGHT
        && !map[new_player.x as usize][new_player.y as usize].blocked
    {
        *player = new_player;
    }

    false
}

fn main() {
    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();

    let con = Offscreen::new(MAP_WIDTH, MAP_HEIGHT);

    let mut tcod = Tcod { root, con };

    // game objects
    let player = Object::new(0, 0, '@', WHITE);
    let npc = Object::new(SCREEN_WIDTH / 2 - 5, SCREEN_HEIGHT / 2, '@', YELLOW);
    let mut objects = [player, npc];

    // game map
    let game = Game {
        map: make_map(&mut objects[0]),
    };

    // game loop
    while !tcod.root.window_closed() {
        tcod.con.clear();

        render_all(&mut tcod, &game.map, &objects);

        tcod.root.flush();

        if handle_keys(&mut tcod, &mut objects[0], &game.map) {
            break;
        }
    }
}
