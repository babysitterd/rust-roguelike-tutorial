use rand::Rng;
use std::cmp;

use tcod::colors::*;
use tcod::console::*;

use tcod::input::{self, Event, Key, Mouse};
use tcod::map::{FovAlgorithm, Map as FovMap};

const SCREEN_WIDTH: i32 = 80;
const SCREEN_HEIGHT: i32 = 50;

const LIMIT_FPS: i32 = 20;

// size of the map
const MAP_WIDTH: i32 = 80;
const MAP_HEIGHT: i32 = 43;

// sizes and coordinates relevant for the GUI
const BAR_WIDTH: i32 = 20;
const PANEL_HEIGHT: i32 = 7;
const PANEL_Y: i32 = SCREEN_HEIGHT - PANEL_HEIGHT;

const MSG_X: i32 = BAR_WIDTH + 2;
const MSG_WIDTH: i32 = SCREEN_WIDTH - BAR_WIDTH - 2;
const MSG_HEIGHT: usize = PANEL_HEIGHT as usize - 1;

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

#[derive(Clone, Copy, Debug, PartialEq)]
enum DeathCallback {
    Player,
    Monster,
}

impl DeathCallback {
    fn callback(self, object: &mut Object, game: &mut Game) {
        use DeathCallback::*;
        let callback: fn(&mut Object, &mut Game) = match self {
            Player => player_death,
            Monster => monster_death,
        };
        callback(object, game);
    }
}

fn player_death(player: &mut Object, game: &mut Game) {
    game.messages.add("You died!", RED);

    player.glyph = '%';
    player.color = DARK_RED;
}

fn monster_death(monster: &mut Object, game: &mut Game) {
    game.messages
        .add(format!("{} is dead!", monster.name), ORANGE);

    monster.glyph = '%';
    monster.color = DARK_RED;
    monster.blocks = false;
    monster.fighter = None;
    monster.ai = None;
    monster.name = format!("remains of {}", monster.name);
}

// combat-related properties and methods (monster, player, NPC).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Fighter {
    max_hp: i32,
    hp: i32,
    defense: i32,
    power: i32,
    on_death: DeathCallback,
}

#[derive(Clone, Debug, PartialEq)]
enum Ai {
    Basic,
}

struct Messages {
    messages: Vec<(String, Color)>,
}

impl Messages {
    pub fn new() -> Self {
        Self { messages: vec![] }
    }

    pub fn add<T: Into<String>>(&mut self, message: T, color: Color) {
        self.messages.push((message.into(), color));
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &(String, Color)> {
        self.messages.iter()
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
    fighter: Option<Fighter>,
    ai: Option<Ai>,
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
            fighter: None,
            ai: None,
        }
    }

    pub fn take_damage(&mut self, damage: i32, game: &mut Game) {
        if let Some(fighter) = self.fighter.as_mut() {
            if damage > 0 {
                fighter.hp = cmp::max(fighter.hp - damage, 0);
            }
            // should not compile due to double-borrowing?
            if self.alive && fighter.hp == 0 {
                self.alive = false;
                fighter.on_death.callback(self, game);
            }
        }
        // if let Some(fighter) = self.fighter {
        //     if self.alive && fighter.hp == 0 {
        //         self.alive = false;
        //         fighter.on_death.callback(self);
        //     }
        // }
    }

    pub fn attack(&self, other: &mut Object, game: &mut Game) {
        if let (Some(off), Some(def)) = (self.fighter, other.fighter) {
            let damage = off.power - def.defense;
            if damage > 0 {
                game.messages.add(
                    format!(
                        "{} attacks {} for {} hit points.",
                        self.name, other.name, damage
                    ),
                    WHITE,
                );
                other.take_damage(damage, game);
            } else {
                game.messages.add(
                    format!("{} attacks {} but it has no effect!", self.name, other.name),
                    WHITE,
                );
            }
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

    pub fn distance_to(&self, other: &Object) -> f32 {
        let dx = other.x - self.x;
        let dy = other.y - self.y;
        ((dx * dx + dy * dy) as f32).sqrt()
    }
}

/// move by the given amount, if the destination is not blocked
fn move_by(id: usize, dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    let pos = objects[id].pos();

    let new_x = pos.0 + dx;
    let new_y = pos.1 + dy;

    if is_blocked(new_x, new_y, &game.map, objects) || is_out_of_bounds(new_x, new_y) {
        return;
    }

    objects[id].set_pos(new_x, new_y);
}

fn player_move_or_attack(dx: i32, dy: i32, game: &mut Game, objects: &mut [Object]) {
    let pos = objects[PLAYER].pos();
    let new_pos = (pos.0 + dx, pos.1 + dy);

    let target_id = objects
        .iter()
        .position(|enemy| enemy.fighter.is_some() && enemy.pos() == new_pos);

    if let Some(id) = target_id {
        let (player, monster) = mut_two(PLAYER, id, objects);
        player.attack(monster, game);
    } else {
        move_by(PLAYER, dx, dy, game, objects);
    }
}

/// Mutably borrow two *separate* elements from the slice
fn mut_two<T>(first_id: usize, second_id: usize, items: &mut [T]) -> (&mut T, &mut T) {
    assert!(first_id < second_id);
    let (first_slice, second_slice) = items.split_at_mut(second_id);
    (&mut first_slice[first_id], &mut second_slice[0])
}

fn ai_take_turn(monster_id: usize, tcod: &Tcod, game: &mut Game, objects: &mut [Object]) {
    let (monster_x, monster_y) = objects[monster_id].pos();
    if tcod.fov.is_in_fov(monster_x, monster_y) {
        if objects[monster_id].distance_to(&objects[PLAYER]) >= 2.0 {
            let (player_x, player_y) = objects[PLAYER].pos();
            move_towards(monster_id, player_x, player_y, game, objects);
        } else if objects[PLAYER].fighter.map_or(false, |f| f.hp >= 0) {
            let (player, monster) = mut_two(PLAYER, monster_id, objects);
            monster.attack(player, game);
        }
    }
}

fn move_towards(id: usize, target_x: i32, target_y: i32, game: &mut Game, objects: &mut [Object]) {
    let dx = target_x - objects[id].x;
    let dy = target_y - objects[id].y;
    let distance = ((dx * dx + dy * dy) as f32).sqrt();

    // normalize it to length 1 (preserving direction), then round it and
    // convert to integer so the movement is restricted to the map grid
    let dx = (dx as f32 / distance).round() as i32;
    let dy = (dy as f32 / distance).round() as i32;
    move_by(id, dx, dy, game, objects);
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
    messages: Messages,
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

fn create_orc(x: i32, y: i32) -> Object {
    let mut orc = Object::new(x, y, 'o', DESATURATED_GREEN, "orc", true);
    orc.alive = true;
    orc.fighter = Some(Fighter {
        max_hp: 10,
        hp: 10,
        defense: 0,
        power: 3,
        on_death: DeathCallback::Monster,
    });
    orc.ai = Some(Ai::Basic);
    orc
}

fn create_troll(x: i32, y: i32) -> Object {
    let mut troll = Object::new(x, y, 'T', DARKER_GREEN, "troll", true);
    troll.alive = true;
    troll.fighter = Some(Fighter {
        max_hp: 16,
        hp: 16,
        defense: 1,
        power: 4,
        on_death: DeathCallback::Monster,
    });
    troll.ai = Some(Ai::Basic);
    troll
}

fn fill_with_monsters(room: &Rectangle, map: &Map, objects: &mut Vec<Object>) {
    let monster_count = rand::thread_rng().gen_range(0, MAX_ROOM_MONSTERS + 1);

    for _ in 0..monster_count {
        let x = rand::thread_rng().gen_range(room.x1 + 1, room.x2);
        let y = rand::thread_rng().gen_range(room.y1 + 1, room.y2);

        if is_blocked(x, y, map, objects) {
            continue;
        }

        let monster = if rand::random::<f32>() < 0.8 {
            // 80% chance of getting an orc
            create_orc(x, y)
        } else {
            create_troll(x, y)
        };

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

fn render_bar(
    panel: &mut Offscreen,
    x: i32,
    y: i32,
    total_width: i32,
    name: &str,
    value: i32,
    maximum: i32,
    bar_color: Color,
    back_color: Color,
) {
    let bar_width = (value as f32 / maximum as f32 * total_width as f32) as i32;

    // render background
    panel.set_default_background(back_color);
    panel.rect(x, y, total_width, 1, false, BackgroundFlag::Screen);

    // render contents on top
    panel.set_default_background(bar_color);
    if bar_width > 0 {
        panel.rect(x, y, bar_width, 1, false, BackgroundFlag::Screen);
    }

    panel.set_default_foreground(WHITE);
    panel.print_ex(
        x + total_width / 2,
        y,
        BackgroundFlag::None,
        TextAlignment::Center,
        format!("{}: {}/{}", name, value, maximum),
    );
}

fn render_all(tcod: &mut Tcod, game: &Game, objects: &[Object]) {
    // render map
    for y in 0..MAP_HEIGHT {
        for x in 0..MAP_WIDTH {
            let tile = &game.map[x as usize][y as usize];
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

    let mut to_draw: Vec<_> = objects
        .iter()
        .filter(|o| tcod.fov.is_in_fov(o.x, o.y))
        .collect();
    to_draw.sort_by(|lhs, rhs| lhs.blocks.cmp(&rhs.blocks));

    // render objects
    for obj in to_draw {
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

    tcod.panel.set_default_background(BLACK);
    tcod.panel.clear();

    let (hp, max_hp) = if let Some(fighter) = objects[PLAYER].fighter {
        (fighter.hp, fighter.max_hp)
    } else {
        (0, 0)
    };

    render_bar(
        &mut tcod.panel,
        1,
        1,
        BAR_WIDTH,
        "HP",
        hp,
        max_hp,
        LIGHT_RED,
        DARKER_RED,
    );

    tcod.panel.set_default_foreground(LIGHT_GREY);
    tcod.panel.print_ex(
        1,
        0,
        BackgroundFlag::None,
        TextAlignment::Left,
        get_names_under_mouse(tcod.mouse, objects, &tcod.fov),
    );

    let mut y = MSG_HEIGHT as i32;
    for &(ref msg, color) in game.messages.iter().rev() {
        let msg_height = tcod.panel.get_height_rect(MSG_X, y, MSG_WIDTH, 0, msg);
        y -= msg_height;
        if y < 0 {
            break;
        }
        tcod.panel.set_default_foreground(color);
        tcod.panel.print_rect(MSG_X, y, MSG_WIDTH, 0, msg);
    }

    blit(
        &tcod.panel,
        (0, 0),
        (SCREEN_WIDTH, SCREEN_HEIGHT),
        &mut tcod.root,
        (0, PANEL_Y),
        1.0,
        1.0,
    );
}

fn get_names_under_mouse(mouse: Mouse, objects: &[Object], fov_map: &FovMap) -> String {
    let (x, y) = (mouse.cx as i32, mouse.cy as i32);

    let names = objects
        .iter()
        .filter(|o| o.pos() == (x, y) && fov_map.is_in_fov(o.x, o.y))
        .map(|o| o.name.clone())
        .collect::<Vec<_>>();

    names.join(", ")
}

struct Tcod {
    root: Root,
    con: Offscreen,
    panel: Offscreen,
    fov: FovMap,
    key: Key,
    mouse: Mouse,
}

fn handle_keys(tcod: &mut Tcod, game: &mut Game, objects: &mut [Object]) -> PlayerAction {
    use tcod::input::Key;
    use tcod::input::KeyCode::*;
    use PlayerAction::*;

    match (tcod.key, tcod.key.text(), objects[PLAYER].alive) {
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
            player_move_or_attack(0, -1, game, objects);
            TookTurn
        }
        (Key { code: Down, .. }, _, true) => {
            player_move_or_attack(0, 1, game, objects);
            TookTurn
        }
        (Key { code: Left, .. }, _, true) => {
            player_move_or_attack(-1, 0, game, objects);
            TookTurn
        }
        (Key { code: Right, .. }, _, true) => {
            player_move_or_attack(1, 0, game, objects);
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
        panel: Offscreen::new(SCREEN_WIDTH, PANEL_HEIGHT),
        fov: FovMap::new(MAP_WIDTH, MAP_HEIGHT),
        key: Default::default(),
        mouse: Default::default(),
    };

    // game objects
    let mut player = Object::new(0, 0, '@', WHITE, "player", true);
    player.alive = true;
    player.fighter = Some(Fighter {
        max_hp: 30,
        hp: 30,
        defense: 2,
        power: 5,
        on_death: DeathCallback::Player,
    });
    let mut objects = vec![player];

    // game map + message log
    let mut game = Game {
        map: make_map(&mut objects),
        messages: Messages::new(),
    };
    game.messages.add(
        "Welcome stranger! Prepare to perish in the Tombs of the Ancient Kings.",
        RED,
    );

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

        match input::check_for_event(input::MOUSE | input::KEY_PRESS) {
            Some((_, Event::Mouse(m))) => tcod.mouse = m,
            Some((_, Event::Key(k))) => tcod.key = k,
            _ => tcod.key = Default::default(),
        }

        tcod.con.clear();
        render_all(&mut tcod, &game, &objects);
        tcod.root.flush();

        previous_player_position = objects[PLAYER].pos();
        let action = handle_keys(&mut tcod, &mut game, &mut objects);
        if action == PlayerAction::Exit {
            break;
        }
        if action != PlayerAction::DidntTakeTurn && objects[PLAYER].alive {
            // only if object is not player
            for id in 1..objects.len() {
                if objects[id].ai.is_some() {
                    ai_take_turn(id, &tcod, &mut game, &mut objects)
                }
            }
        }
    }
}
