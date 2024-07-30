use rand::Rng;
use std::cmp;

use tcod::colors::*;
use tcod::console::*;

use tcod::map::{FovAlgorithm, Map as FovMap};

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

const FOV_ALGO: FovAlgorithm = FovAlgorithm::Basic; // default FOV algorithm
const FOV_LIGHT_WALLS: bool = true; // light walls or not
const TORCH_RADIUS: i32 = 10;

const COLOR_DARK_WALL: Color = Color { r: 0, g: 0, b: 100 };
const COLOR_LIGHT_WALL: Color = Color {
    r: 130,
    g: 110,
    b: 50,
};
const COLOR_DARK_GROUND: Color = Color {
    r: 50,
    g: 50,
    b: 150,
};
const COLOR_LIGHT_GROUND: Color = Color {
    r: 200,
    g: 180,
    b: 50,
};

const MAX_ROOM_MONSTERS: i32 = 3;

// player will always be the first object
const PLAYER: usize = 0;

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
    glyph: char,
    color: Color,
    name: String,
    blocks: bool,
    alive: bool,
}

impl Object {
    pub fn new(x: i32, y: i32, glyph: char, color: Color, name: &str, blocks: bool) -> Self {
        Object {
            x: x,
            y: y,
            glyph: glyph,
            color: color,
            name: name.into(),
            blocks: blocks,
            alive: false,
        }
    }

    /// set the color and then draw the character that represents this object at its position
    pub fn draw(&self, con: &mut dyn Console) {
        con.set_default_foreground(self.color);
        con.put_char(self.x, self.y, self.glyph, BackgroundFlag::None);
    }

    pub fn pos(&self) -> (i32, i32) {
        return (self.x, self.y);
    }

    pub fn set_pos(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }
}

/// move by the given amount, if the destination is not blocked
fn move_by(id: usize, dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let pos = objects[id].pos();

    let new_x = pos.0 + dx;
    let new_y = pos.1 + dy;

    if is_blocked(new_x, new_y, map, objects) || is_out_of_bounds(new_x, new_y) {
        return;
    }

    objects[id].set_pos(new_x, new_y);
}

fn player_move_or_attack(dx: i32, dy: i32, map: &Map, objects: &mut [Object]) {
    let pos = objects[PLAYER].pos();
    let new_pos = (pos.0 + dx, pos.1 + dy);

    let target_id = objects.iter().position(|enemy| enemy.pos() == new_pos);

    if let Some(id) = target_id {
        println!(
            "The {} laughs at your puny efforts to attack him!",
            objects[id].name
        );
    } else {
        move_by(PLAYER, dx, dy, map, objects);
    }
}

/// A tile of the map and its properties
#[derive(Clone, Copy, Debug)]
struct Tile {
    blocked: bool,
    explored: bool,
    block_sight: bool,
}

impl Tile {
    pub fn empty() -> Self {
        Tile {
            blocked: false,
            explored: false,
            block_sight: false,
        }
    }

    pub fn wall() -> Self {
        Tile {
            blocked: true,
            explored: false,
            block_sight: true,
        }
    }
}

type Map = Vec<Vec<Tile>>;

struct Game {
    map: Map,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PlayerAction {
    TookTurn,
    DidntTakeTurn,
    Exit,
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

fn make_map(objects: &mut Vec<Object>) -> Map {
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
                let player = &mut objects[PLAYER];
                player.set_pos(new_x, new_y);
            }

            rooms.push(Rectangle::new(x, y, w, h));
        }
    }

    for room in &rooms {
        carve_room(room, &mut map);
        fill_with_monsters(&room, &map, objects);
    }

    map
}

fn is_blocked(x: i32, y: i32, map: &Map, objects: &[Object]) -> bool {
    if map[x as usize][y as usize].blocked {
        return true;
    }

    objects
        .iter()
        .any(|object| object.pos() == (x, y) && object.blocks)
}

fn is_out_of_bounds(x: i32, y: i32) -> bool {
    x < 0 || x >= MAP_WIDTH || y < 0 || y >= MAP_HEIGHT
}

fn fill_with_monsters(room: &Rectangle, map: &Map, objects: &mut Vec<Object>) {
    let monster_count = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

    for _ in 0..monster_count {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if is_blocked(x, y, map, objects) {
            println!("can't place into {} {} as it's already blocked", x, y);
            continue;
        }

        let mut monster = if rand::random::<f32>() < 0.8 {
            // 80% chance of getting an orc
            Object::new(x, y, 'o', DESATURATED_GREEN, "orc", true)
        } else {
            Object::new(x, y, 'T', DARKER_GREEN, "troll", true)
        };

        monster.alive = true;
        objects.push(monster);
    }
}

fn vision_update(tcod: &mut Tcod, map: &mut Map, player: &Object) {
    // recompute fov
    tcod.fov
        .compute_fov(player.x, player.y, TORCH_RADIUS, FOV_LIGHT_WALLS, FOV_ALGO);

    // explore map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            if tcod.fov.is_in_fov(x, y) {
                map[x as usize][y as usize].explored = true;
            }
        }
    }
}

fn render_all(tcod: &mut Tcod, map: &Map, objects: &[Object]) {
    // render map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let tile = &map[x as usize][y as usize];
            let wall = tile.block_sight;
            let lit = tcod.fov.is_in_fov(x, y);
            let color = match (wall, lit) {
                (true, true) => COLOR_LIGHT_WALL,
                (true, false) => COLOR_DARK_WALL,
                (false, true) => COLOR_LIGHT_GROUND,
                (false, false) => COLOR_DARK_GROUND,
            };
            if tile.explored {
                tcod.con
                    .set_char_background(x, y, color, BackgroundFlag::Set);
            }
        }
    }

    // render objects
    for obj in objects {
        if tcod.fov.is_in_fov(obj.x, obj.y) {
            obj.draw(&mut tcod.con);
        }
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
    fov: FovMap,
}

fn handle_keys(tcod: &mut Tcod, map: &Map, objects: &mut [Object]) -> PlayerAction {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    let key = tcod.root.wait_for_keypress(true);
    match (key, key.text(), objects[PLAYER].alive) {
        (
            Key {
                code: Enter,
                alt: true,
                ..
            },
            _,
            _,
        ) => {
            // Alt+Enter: toggle fullscreen
            let fullscreen = tcod.root.is_fullscreen();
            tcod.root.set_fullscreen(!fullscreen);
            return DidntTakeTurn;
        }
        (Key { code: Escape, .. }, _, _) => return Exit, // exit game

        // movement keys
        (Key { code: Up, .. }, _, true) => {
            player_move_or_attack(0, -1, map, objects);
            TookTurn
        }
        (Key { code: Down, .. }, _, true) => {
            player_move_or_attack(0, 1, map, objects);
            TookTurn
        }
        (Key { code: Left, .. }, _, true) => {
            player_move_or_attack(-1, 0, map, objects);
            TookTurn
        }
        (Key { code: Right, .. }, _, true) => {
            player_move_or_attack(1, 0, map, objects);
            TookTurn
        }

        // etc
        _ => return DidntTakeTurn,
    }
}

fn main() {
    tcod::system::set_fps(LIMIT_FPS);

    let root = Root::initializer()
        .font("arial10x10.png", FontLayout::Tcod)
        .font_type(FontType::Greyscale)
        .size(SCREEN_WIDTH, SCREEN_HEIGHT)
        .title("Rust/libtcod tutorial")
        .init();

    let mut tcod = Tcod {
        root,
        con: Offscreen::new(MAP_WIDTH, MAP_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
    };

    // game objects
    let mut player = Object::new(0, 0, '@', WHITE, "player", true);
    player.alive = true;
    let mut objects = vec![player];

    // game map
    let mut game = Game {
        map: make_map(&mut objects),
    };

    // fov
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let tile = &game.map[x as usize][y as usize];
            tcod.fov.set(x, y, !tile.block_sight, !tile.blocked);
        }
    }

    // game loop
    let mut previous_player_position = (-1, -1);
    while !tcod.root.window_closed() {
        if objects[PLAYER].pos() != previous_player_position {
            vision_update(&mut tcod, &mut game.map, &objects[PLAYER]);
        }

        tcod.con.clear();
        render_all(&mut tcod, &game.map, &objects);
        tcod.root.flush();

        previous_player_position = objects[PLAYER].pos();
        let action = handle_keys(&mut tcod, &game.map, &mut objects);
        if action == PlayerAction::Exit {
            break;
        }
        if action != PlayerAction::DidntTakeTurn && objects[PLAYER].alive {
            // only if object is not player
            for i in 1..objects.len() {
                println!("The {} growls!", objects[i].name);
            }
        }
    }
}
